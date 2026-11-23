use composer_format::container::{encode, SectionSpec};
use composer_format::{
    peek, read_project, write_project, AssetRef, AssetTable, ContentHash, FormatError, Limits,
    Meta, SectionIndex, Tag, FORMAT_VERSION,
};
use composer_model::{
    Change, Id, Note, Project, Region, RegionContent, Track, TrackKind, MAX_TICK,
};

fn id(n: u128) -> Id {
    Id(n)
}

fn note(n: u128, pitch: u8, start: i64) -> Note {
    Note {
        id: id(n),
        pitch,
        velocity: 90,
        start,
        duration: 240,
    }
}

fn midi_region(rid: u128, notes: Vec<Note>) -> Region {
    Region {
        loop_length: 0,
        id: id(rid),
        start: 0,
        length: 3840,
        content: RegionContent::Midi {
            notes: notes.into_iter().map(|n| (n.id, n)).collect(),
        },
    }
}

fn track(tid: u128, kind: TrackKind, regions: Vec<Region>) -> Track {
    Track {
        id: id(tid),
        name: format!("track {tid}"),
        kind,
        instrument: (kind == TrackKind::Instrument).then_some(id(0x99)),
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: regions.into_iter().map(|r| (r.id, r)).collect(),
    }
}

fn add(p: &mut Project, t: Track) {
    let index = p.tracks.len();
    p.apply(&Change::NewTrack { index, track: t }).unwrap();
}

fn small() -> Project {
    let mut p = Project::new(id(1), "small demo");
    p.apply(&Change::SetTempo {
        tick: 3840,
        old: None,
        new: Some(90_000),
    })
    .unwrap();
    let r = midi_region(0x20, vec![note(0x30, 60, 0), note(0x31, 64, 480)]);
    add(&mut p, track(0x10, TrackKind::Instrument, vec![r]));
    p
}

fn with_audio() -> (Project, AssetTable) {
    let mut p = small();
    let audio = Region {
        loop_length: 0,
        id: id(0x21),
        start: 960,
        length: 7680,
        content: RegionContent::Audio,
    };
    add(&mut p, track(0x11, TrackKind::Audio, vec![audio]));
    let mut t = AssetTable::new();
    for (n, name) in [(0xa2u128, "kick.wav"), (0xa1, "pad.flac")] {
        t.insert(AssetRef {
            id: id(n),
            hash: ContentHash::of(name.as_bytes()),
            bytes: 1_000_000 + n as u64,
            name: name.into(),
            sample_rate: 48_000,
            channels: 2,
        })
        .unwrap();
    }
    (p, t)
}

fn meta() -> Meta {
    Meta {
        app_version: "0.0.1".into(),
        created_ms: 1_700_000_000_000,
        modified_ms: 1_700_000_500_000,
        transport: None,
    }
}

fn limits() -> Limits {
    Limits::default()
}

fn golden(name: &str, bytes: &[u8]) {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("COMPOSER_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, bytes).unwrap();
    }
    let want = std::fs::read(&path).expect("golden file exists");
    assert!(
        want == bytes,
        "{name} differs from the committed golden file; the format changed"
    );
}

fn small_bytes() -> Vec<u8> {
    write_project(&small(), &AssetTable::new(), &meta())
}

fn assets_bytes() -> Vec<u8> {
    let (p, t) = with_audio();
    write_project(&p, &t, &meta())
}

/// Same sections as a normal file plus one extra section.
fn with_extra(bytes: &[u8], required: bool) -> Vec<u8> {
    let ix = SectionIndex::parse(bytes, &limits()).unwrap();
    let mut datas: Vec<(Tag, Vec<u8>)> = ix
        .entries()
        .iter()
        .map(|e| (e.tag, ix.section(bytes, e).unwrap().to_vec()))
        .collect();
    datas.insert(1, (Tag(*b"xtra"), b"future data".to_vec()));
    let specs: Vec<SectionSpec> = datas
        .iter()
        .map(|(tag, data)| SectionSpec {
            tag: *tag,
            required: required || *tag != Tag(*b"xtra"),
            key: 0,
            data,
        })
        .collect();
    encode(FORMAT_VERSION, &specs)
}

#[test]
fn golden_files_pin_the_bytes() {
    golden("small.cmpf", &small_bytes());
    golden("assets.cmpf", &assets_bytes());
    golden("optional_section.cmpf", &with_extra(&small_bytes(), false));
}

#[test]
fn golden_files_still_load() {
    for name in ["small.cmpf", "assets.cmpf", "optional_section.cmpf"] {
        let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(path).unwrap();
        let l = read_project(&bytes, &limits()).unwrap();
        assert_eq!(l.version, 0);
        assert_eq!(l.meta, meta());
    }
}

#[test]
fn round_trip_keeps_state_hash_assets_and_meta() {
    let (p, t) = with_audio();
    let bytes = write_project(&p, &t, &meta());
    let l = read_project(&bytes, &limits()).unwrap();
    assert_eq!(l.project, p);
    assert_eq!(l.project.state_hash(), p.state_hash());
    assert!(l.project.index_is_consistent());
    assert_eq!(l.assets, t);
    assert_eq!(l.meta, meta());
    // Writing is deterministic and stable through a second trip.
    assert_eq!(write_project(&l.project, &l.assets, &l.meta), bytes);
}

#[test]
fn effect_chains_survive_a_save_and_load() {
    use composer_model::{Effect, EffectKind, EffectOwner};
    let mut p = small();
    let track = p.tracks[0].id;
    for (n, owner) in [
        (900, EffectOwner::Track { track }),
        (901, EffectOwner::Master),
    ] {
        p.apply(&Change::InsertEffect {
            owner,
            index: 0,
            effect: Effect::new(id(n), EffectKind::Delay),
        })
        .unwrap();
    }
    let bytes = write_project(&p, &AssetTable::default(), &meta());
    let l = read_project(&bytes, &limits()).unwrap();
    assert_eq!(l.project, p);
    assert_eq!(l.project.state_hash(), p.state_hash());
    assert!(l.project.index_is_consistent());
}

#[test]
fn optional_unknown_section_is_skipped_required_is_rejected() {
    let base = small_bytes();
    let l = read_project(&with_extra(&base, false), &limits()).unwrap();
    assert_eq!(l.project, small());
    assert_eq!(l.skipped_sections, vec![Tag(*b"xtra")]);
    let e = read_project(&with_extra(&base, true), &limits()).unwrap_err();
    assert_eq!(
        e,
        FormatError::UnknownRequiredSection { tag: Tag(*b"xtra") }
    );
    assert_eq!(e.code(), "unknown_required_section");
}

#[test]
fn every_proper_prefix_is_rejected() {
    for bytes in [
        small_bytes(),
        assets_bytes(),
        with_extra(&small_bytes(), false),
    ] {
        for n in 0..bytes.len() {
            let e = read_project(&bytes[..n], &limits()).unwrap_err();
            assert!(
                matches!(e, FormatError::Truncated { .. } | FormatError::BadMagic),
                "prefix {n}: {e:?}"
            );
            assert!(peek(&bytes[..n]).is_err());
        }
    }
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut b = small_bytes();
    b.push(0);
    assert_eq!(read_project(&b, &limits()), Err(FormatError::TrailingBytes));
}

#[test]
fn flipped_byte_in_each_section_names_that_section() {
    let bytes = assets_bytes();
    let ix = SectionIndex::parse(&bytes, &limits()).unwrap();
    assert_eq!(ix.entries().len(), 3);
    for e in ix.entries() {
        let start = e.offset as usize;
        let end = start + e.len as usize;
        for at in [start, (start + end) / 2, end - 1] {
            let mut b = bytes.clone();
            b[at] ^= 0x01;
            let err = read_project(&b, &limits()).unwrap_err();
            assert_eq!(err, FormatError::SectionCorrupt { tag: e.tag }, "byte {at}");
            assert_eq!(err.section(), Some(e.tag));
        }
    }
}

#[test]
fn flipped_byte_in_manifest_is_a_manifest_error() {
    let bytes = assets_bytes();
    let ix = SectionIndex::parse(&bytes, &limits()).unwrap();
    for at in 8..ix.manifest_bytes() {
        let mut b = bytes.clone();
        b[at] ^= 0x80;
        let e = read_project(&b, &limits()).unwrap_err();
        assert!(
            matches!(
                e,
                FormatError::ManifestCorrupt
                    | FormatError::UnsupportedVersion { .. }
                    | FormatError::BadHeader(_)
                    | FormatError::LimitExceeded { .. }
                    | FormatError::Truncated { .. }
            ),
            "byte {at}: {e:?}"
        );
    }
    let mut b = bytes.clone();
    b[0] ^= 1;
    assert_eq!(read_project(&b, &limits()), Err(FormatError::BadMagic));
}

#[test]
fn future_version_is_typed_with_the_version_found() {
    let mut b = small_bytes();
    b[8..10].copy_from_slice(&7u16.to_le_bytes());
    let e = read_project(&b, &limits()).unwrap_err();
    assert_eq!(
        e,
        FormatError::UnsupportedVersion {
            found: 7,
            supported: FORMAT_VERSION
        }
    );
    assert_eq!(e.code(), "unsupported_version");
}

#[test]
fn missing_and_duplicate_sections_are_rejected() {
    let base = small_bytes();
    let ix = SectionIndex::parse(&base, &limits()).unwrap();
    let proj = ix.section(&base, ix.entry(Tag::PROJECT).unwrap()).unwrap();
    let spec = |tag| SectionSpec {
        tag,
        required: true,
        key: 0,
        data: proj,
    };
    let only_proj = encode(FORMAT_VERSION, &[spec(Tag::PROJECT)]);
    assert_eq!(
        read_project(&only_proj, &limits()),
        Err(FormatError::MissingSection { tag: Tag::ASSETS })
    );
    let twice = encode(FORMAT_VERSION, &[spec(Tag::PROJECT), spec(Tag::PROJECT)]);
    assert_eq!(
        read_project(&twice, &limits()),
        Err(FormatError::DuplicateSection { tag: Tag::PROJECT })
    );
}

/// A file with valid checksums whose project section holds `json`.
fn file_with_project_json(json: &[u8]) -> Vec<u8> {
    let base = small_bytes();
    let ix = SectionIndex::parse(&base, &limits()).unwrap();
    let specs: Vec<SectionSpec> = ix
        .entries()
        .iter()
        .map(|e| SectionSpec {
            tag: e.tag,
            required: true,
            key: 0,
            data: if e.tag == Tag::PROJECT {
                json
            } else {
                ix.section(&base, e).unwrap()
            },
        })
        .collect();
    encode(FORMAT_VERSION, &specs)
}

#[test]
fn duplicate_ids_are_rejected() {
    // Same id on two tracks.
    let mut p = small();
    let mut dup = p.tracks[0].clone();
    dup.regions.clear();
    p.tracks.push(dup);
    let b = write_project(&p, &AssetTable::new(), &meta());
    assert_eq!(
        read_project(&b, &limits()),
        Err(FormatError::DuplicateId(id(0x10)))
    );

    // A note id reused in another track's region.
    let mut p = small();
    let r = midi_region(0x40, vec![note(0x30, 62, 0)]);
    p.tracks.push(track(0x12, TrackKind::Instrument, vec![r]));
    let b = write_project(&p, &AssetTable::new(), &meta());
    assert_eq!(
        read_project(&b, &limits()),
        Err(FormatError::DuplicateId(id(0x30)))
    );

    // Track reusing the project id.
    let mut p = small();
    p.tracks[0].id = id(1);
    let b = write_project(&p, &AssetTable::new(), &meta());
    assert_eq!(
        read_project(&b, &limits()),
        Err(FormatError::DuplicateId(id(1)))
    );
}

#[test]
fn dangling_key_is_rejected() {
    let mut p = small();
    let r = p.tracks[0].regions.remove(&id(0x20)).unwrap();
    p.tracks[0].regions.insert(id(0x77), r);
    let b = write_project(&p, &AssetTable::new(), &meta());
    let e = read_project(&b, &limits()).unwrap_err();
    assert_eq!(e.code(), "dangling_reference");
}

#[test]
fn out_of_range_and_inconsistent_values_are_rejected() {
    type Edit = Box<dyn Fn(&mut Project)>;
    let cases: Vec<(&str, Edit)> = vec![
        ("velocity", Box::new(|p| edit_note(p, |n| n.velocity = 0))),
        ("pitch", Box::new(|p| edit_note(p, |n| n.pitch = 200))),
        (
            "tick",
            Box::new(|p| edit_note(p, |n| n.start = MAX_TICK + 1)),
        ),
        (
            "outside region",
            Box::new(|p| edit_note(p, |n| n.start = 3800)),
        ),
        ("name", Box::new(|p| p.name = "x".repeat(300))),
        (
            "tempo",
            Box::new(|p| {
                p.tempo_map.tempos.insert(960, 5);
            }),
        ),
        (
            "no tick 0",
            Box::new(|p| {
                p.tempo_map.tempos.remove(&0);
            }),
        ),
        (
            "audio region on instrument track",
            Box::new(|p| {
                let r = Region {
                    loop_length: 0,
                    id: id(0x50),
                    start: 0,
                    length: 10,
                    content: RegionContent::Audio,
                };
                p.tracks[0].regions.insert(r.id, r);
            }),
        ),
    ];
    for (label, f) in cases {
        let mut p = small();
        f(&mut p);
        let b = write_project(&p, &AssetTable::new(), &meta());
        let e = read_project(&b, &limits()).unwrap_err();
        assert!(
            matches!(e.code(), "out_of_range" | "inconsistent"),
            "{label}: {e:?}"
        );
    }
}

fn edit_note(p: &mut Project, f: impl Fn(&mut Note)) {
    for r in p.tracks[0].regions.values_mut() {
        if let RegionContent::Midi { notes } = &mut r.content {
            for n in notes.values_mut() {
                f(n);
            }
        }
    }
}

#[test]
fn bad_asset_tables_are_rejected() {
    let base = assets_bytes();
    let ix = SectionIndex::parse(&base, &limits()).unwrap();
    let tweak = |json: &str| {
        let specs: Vec<SectionSpec> = ix
            .entries()
            .iter()
            .map(|e| SectionSpec {
                tag: e.tag,
                required: true,
                key: 0,
                data: if e.tag == Tag::ASSETS {
                    json.as_bytes()
                } else {
                    ix.section(&base, e).unwrap()
                },
            })
            .collect();
        encode(FORMAT_VERSION, &specs)
    };
    let hash = "0".repeat(64);
    let one = |extra: &str| {
        format!(
            r#"[{{"id":"{:032x}","hash":"{hash}","bytes":1,"name":"a","sample_rate":44100,"channels":2{extra}}}]"#,
            5
        )
    };
    assert!(read_project(&tweak(&one("")), &limits()).is_ok());
    let dup = format!(
        "[{},{}]",
        &one("")[1..one("").len() - 1],
        &one("")[1..one("").len() - 1]
    );
    assert_eq!(
        read_project(&tweak(&dup), &limits()),
        Err(FormatError::DuplicateId(id(5)))
    );
    let bad_rate = one("").replace("44100", "0");
    assert_eq!(
        read_project(&tweak(&bad_rate), &limits())
            .unwrap_err()
            .code(),
        "out_of_range"
    );
    let bad_hash = one("").replace(&hash, "XYZ");
    assert_eq!(
        read_project(&tweak(&bad_hash), &limits())
            .unwrap_err()
            .code(),
        "malformed"
    );
}

#[test]
fn limits_are_enforced() {
    let bytes = assets_bytes();
    let tight = |f: fn(&mut Limits)| {
        let mut l = limits();
        f(&mut l);
        read_project(&bytes, &l).unwrap_err()
    };
    assert_eq!(tight(|l| l.max_file_bytes = 100).code(), "limit_exceeded");
    assert_eq!(tight(|l| l.max_sections = 2).code(), "limit_exceeded");
    assert_eq!(
        tight(|l| l.max_project_bytes = 10).code(),
        "section_too_large"
    );
    assert_eq!(
        tight(|l| l.max_assets_bytes = 10).code(),
        "section_too_large"
    );
    assert_eq!(tight(|l| l.max_meta_bytes = 10).code(), "section_too_large");
    assert_eq!(tight(|l| l.max_tracks = 1).code(), "limit_exceeded");
    assert_eq!(tight(|l| l.max_regions = 1).code(), "limit_exceeded");
    assert_eq!(tight(|l| l.max_notes = 1).code(), "limit_exceeded");
    assert_eq!(tight(|l| l.max_assets = 1).code(), "limit_exceeded");
    let extra = with_extra(&small_bytes(), false);
    let mut l = limits();
    l.max_other_section_bytes = 3;
    assert_eq!(
        read_project(&extra, &l).unwrap_err().code(),
        "section_too_large"
    );
}

#[test]
fn huge_claimed_lengths_never_allocate() {
    // A manifest claiming a 2^60 byte section: rejected by limits, then by extent.
    let data = [0u8; 4];
    let bytes = encode(
        FORMAT_VERSION,
        &[SectionSpec {
            tag: Tag(*b"xtra"),
            required: false,
            key: 0,
            data: &data,
        }],
    );
    // Rewrite length and refresh the manifest hash so only the limits stand in the way.
    let mut b = bytes.clone();
    let len_at = 16 + 32;
    b[len_at..len_at + 8].copy_from_slice(&(1u64 << 60).to_le_bytes());
    let mlen = 16 + 72 + 32;
    let h = composer_format::sha256::sha256(&b[..mlen - 32]);
    b[mlen - 32..mlen].copy_from_slice(&h);
    let e = read_project(&b, &limits()).unwrap_err();
    assert_eq!(e.code(), "section_too_large");
    let mut l = limits();
    l.max_other_section_bytes = u64::MAX;
    l.max_file_bytes = u64::MAX;
    assert_eq!(read_project(&b, &l).unwrap_err().code(), "truncated");
    // A section count of u32::MAX with a tiny file.
    let mut b = bytes;
    b[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        read_project(&b, &limits()).unwrap_err().code(),
        "limit_exceeded"
    );
    l.max_sections = u32::MAX;
    assert_eq!(read_project(&b, &l).unwrap_err().code(), "truncated");
}

#[test]
fn section_index_reads_only_the_manifest() {
    let bytes = assets_bytes();
    let ix = SectionIndex::parse(&bytes, &limits()).unwrap();
    // The manifest alone (no section data) is enough.
    let prefix = &bytes[..ix.manifest_bytes()];
    let ix2 = SectionIndex::parse(prefix, &limits()).unwrap();
    assert_eq!(ix, ix2);
    assert_eq!(ix.total_len(), bytes.len() as u64);
    assert_eq!(
        SectionIndex::manifest_len(&bytes[..16], &limits()).unwrap(),
        ix.manifest_bytes()
    );
    let tags: Vec<Tag> = ix.entries().iter().map(|e| e.tag).collect();
    assert_eq!(tags, [Tag::PROJECT, Tag::ASSETS, Tag::META]);
    // But reading the whole file from the prefix fails.
    assert_eq!(
        read_project(prefix, &limits()).unwrap_err().code(),
        "truncated"
    );
}

#[test]
fn peek_reports_manifest_level_facts() {
    let (p, t) = with_audio();
    let b = write_project(&p, &t, &meta());
    let info = peek(&b).unwrap();
    let ix = SectionIndex::parse(&b, &limits()).unwrap();
    assert_eq!(info.name.as_deref(), Some("small demo"));
    assert_eq!(info.version, 0);
    assert_eq!(info.file_bytes, b.len() as u64);
    assert_eq!(info.project_bytes, ix.entry(Tag::PROJECT).unwrap().len);
    assert_eq!(info.assets_bytes, ix.entry(Tag::ASSETS).unwrap().len);
    assert_eq!(info.meta, Some(meta()));
}

#[test]
fn peek_skips_project_checksum_but_fails_closed_elsewhere() {
    let (p, t) = with_audio();
    let b = write_project(&p, &t, &meta());
    let ix = SectionIndex::parse(&b, &limits()).unwrap();
    let proj = ix.entry(Tag::PROJECT).unwrap();
    // Corruption deep inside proj is outside what peek promises; read_project catches it.
    let mut bad = b.clone();
    let deep = (proj.offset + proj.len - 2) as usize;
    bad[deep] ^= 1;
    assert!(peek(&bad).is_ok());
    assert_eq!(
        read_project(&bad, &limits()).unwrap_err().code(),
        "section_corrupt"
    );
    // The meta section is verified.
    let m = ix.entry(Tag::META).unwrap();
    let mut bad = b.clone();
    bad[m.offset as usize] ^= 1;
    assert_eq!(peek(&bad).unwrap_err().code(), "section_corrupt");
    // So are the extent and the manifest.
    let mut long = b.clone();
    long.push(0);
    assert_eq!(peek(&long).unwrap_err().code(), "trailing_bytes");
    let mut bad = b.clone();
    bad[10] ^= 1;
    assert!(peek(&bad).is_err());
}

#[test]
fn peek_ignores_project_bytes_beyond_the_name_prefix() {
    let mut p = Project::new(id(1), "big");
    for t in 0..200 {
        add(&mut p, track(10_000 + t, TrackKind::Instrument, vec![]));
    }
    let b = write_project(&p, &AssetTable::new(), &meta());
    let ix = SectionIndex::parse(&b, &limits()).unwrap();
    let proj = ix.entry(Tag::PROJECT).unwrap();
    assert!(proj.len > 8192);
    let want = peek(&b).unwrap();
    assert_eq!(want.name.as_deref(), Some("big"));
    let mut b2 = b.clone();
    let from = proj.offset as usize + 5000;
    for x in &mut b2[from..from + 100] {
        *x = b'#';
    }
    assert_eq!(peek(&b2).unwrap(), want);
    assert!(read_project(&b2, &limits()).is_err());
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let seeds = [
        small_bytes(),
        assets_bytes(),
        with_extra(&small_bytes(), false),
    ];
    for i in 0..3000 {
        let mut b = seeds[i % seeds.len()].clone();
        for _ in 0..=(rng.next() % 4) {
            let at = (rng.next() as usize) % b.len();
            b[at] = rng.next() as u8;
        }
        if rng.next().is_multiple_of(5) {
            b.truncate((rng.next() as usize) % b.len());
        }
        let _ = read_project(&b, &limits());
        let _ = peek(&b);
        let _ = SectionIndex::parse(&b, &limits());
    }
    for _ in 0..500 {
        let n = (rng.next() % 300) as usize;
        let mut b: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        if n >= 8 && rng.next().is_multiple_of(2) {
            b[..8].copy_from_slice(&composer_format::container::MAGIC);
        }
        let _ = read_project(&b, &limits());
        let _ = peek(&b);
    }
}

#[test]
fn garbage_inside_checksummed_sections_never_panics() {
    // Valid checksums over mutated JSON reach the parsers and validators.
    let mut rng = Rng(0xfeed_beef_cafe_0001);
    let (p, t) = with_audio();
    let proj = serde_json::to_vec(&p).unwrap();
    for _ in 0..3000 {
        let mut j = proj.clone();
        for _ in 0..=(rng.next() % 3) {
            let at = (rng.next() as usize) % j.len();
            j[at] = b"{}[]\":,0123456789abcdefx -"[(rng.next() % 26) as usize];
        }
        let b = file_with_project_json(&j);
        let _ = read_project(&b, &limits());
        let _ = peek(&b);
    }
    let _ = t;
}

/// The previous validation path, kept as a reference: replay the project through `apply`.
fn replay_accepts(parsed: &Project) -> bool {
    let mut fresh = Project::new(parsed.id, parsed.name.clone());
    for (&tick, &v) in &parsed.tempo_map.tempos {
        let old = fresh.tempo_map.tempos.get(&tick).copied();
        let c = Change::SetTempo {
            tick,
            old,
            new: Some(v),
        };
        if fresh.apply(&c).is_err() {
            return false;
        }
    }
    for (&tick, &v) in &parsed.tempo_map.meters {
        let old = fresh.tempo_map.meters.get(&tick).copied();
        let c = Change::SetMeter {
            tick,
            old,
            new: Some(v),
        };
        if fresh.apply(&c).is_err() {
            return false;
        }
    }
    if fresh.tempo_map != parsed.tempo_map || parsed.name.len() > composer_model::MAX_NAME_LEN {
        return false;
    }
    for (index, track) in parsed.tracks.iter().enumerate() {
        if track.regions.iter().any(|(k, r)| *k != r.id) {
            return false;
        }
        let change = Change::NewTrack {
            index,
            track: track.clone(),
        };
        if fresh.apply(&change).is_err() {
            return false;
        }
    }
    fresh.index_is_consistent()
}

#[test]
fn validate_verdicts_match_the_old_replay_on_fuzz_inputs() {
    let mut rng = Rng(0x0dd_ba11_5eed_0003);
    let (p, _) = with_audio();
    let proj = serde_json::to_vec(&p).unwrap();
    let (mut accepted, mut rejected) = (0, 0);
    for _ in 0..4000 {
        let mut j = proj.clone();
        for _ in 0..=(rng.next() % 3) {
            let at = (rng.next() as usize) % j.len();
            j[at] = b"{}[]\":,0123456789abcdefx -"[(rng.next() % 26) as usize];
        }
        let Ok(parsed) = serde_json::from_slice::<Project>(&j) else {
            continue;
        };
        let new = parsed.validate().is_ok();
        assert_eq!(
            new,
            replay_accepts(&parsed),
            "{}",
            String::from_utf8_lossy(&j)
        );
        if new {
            accepted += 1;
        } else {
            rejected += 1;
        }
    }
    assert!(accepted > 20 && rejected > 100, "{accepted} {rejected}");
}

#[test]
fn transport_prefs_round_trip_in_meta_and_default_files_stay_byte_identical() {
    use composer_format::TransportPrefs;
    let p = small();
    let plain = write_project(&p, &AssetTable::new(), &meta());
    assert!(!String::from_utf8_lossy(&plain).contains("count_in_bars"));
    let mut prefs = TransportPrefs::default();
    prefs.loop_range.on = true;
    prefs.loop_range.end = 3840;
    prefs.count_in_bars = 1;
    let m = Meta {
        transport: Some(prefs),
        ..meta()
    };
    let bytes = write_project(&p, &AssetTable::new(), &m);
    let loaded = read_project(&bytes, &limits()).unwrap();
    assert_eq!(loaded.meta.transport, Some(prefs));
    assert_eq!(loaded.project.state_hash(), p.state_hash());
    assert_eq!(
        read_project(&plain, &limits()).unwrap().meta.transport,
        None
    );
    // A bad stored loop fails closed.
    let mut bad = prefs;
    bad.loop_range.start = 9999;
    let m = Meta {
        transport: Some(bad),
        ..meta()
    };
    let bytes = write_project(&p, &AssetTable::new(), &m);
    assert!(read_project(&bytes, &limits()).is_err());
}

#[test]
fn master_and_send_round_trip_and_default_files_omit_them() {
    use composer_model::{FieldKey, FieldValue, MasterMix};
    let plain = String::from_utf8_lossy(&small_bytes()).into_owned();
    assert!(!plain.contains("reverb_send") && !plain.contains("reverb_mix"));
    let mut p = small();
    let tid = p.tracks[0].id;
    p.apply(&Change::Set {
        key: FieldKey::Master,
        old: FieldValue::Master(p.master),
        new: FieldValue::Master(MasterMix {
            volume: 900,
            reverb_size: 650,
            reverb_mix: 220,
        }),
    })
    .unwrap();
    p.apply(&Change::Set {
        key: FieldKey::TrackReverbSend { track: tid },
        old: FieldValue::Permille(0),
        new: FieldValue::Permille(350),
    })
    .unwrap();
    let bytes = write_project(&p, &AssetTable::new(), &meta());
    let back = read_project(&bytes, &limits()).unwrap().project;
    assert_eq!(back, p);
    assert_eq!(back.state_hash(), p.state_hash());
    assert_eq!(back.master.reverb_mix, 220);
    assert_eq!(back.tracks[0].reverb_send, 350);
}

#[test]
fn loop_and_markers_round_trip_and_default_files_omit_them() {
    use composer_model::{FieldKey, FieldValue};
    let plain = String::from_utf8_lossy(&small_bytes()).into_owned();
    assert!(!plain.contains("loop_length") && !plain.contains("markers"));
    let mut p = small();
    let rid = p
        .tracks
        .iter()
        .flat_map(|t| t.regions.values())
        .find(|r| matches!(r.content, RegionContent::Midi { .. }))
        .unwrap()
        .id;
    p.apply(&Change::Set {
        key: FieldKey::RegionLoop { region: rid },
        old: FieldValue::Tick(0),
        new: FieldValue::Tick(960),
    })
    .unwrap();
    p.apply(&Change::SetMarker {
        tick: 1920,
        old: None,
        new: Some("chorus".into()),
    })
    .unwrap();
    let bytes = write_project(&p, &AssetTable::new(), &meta());
    let back = read_project(&bytes, &limits()).unwrap().project;
    assert_eq!(back, p);
    assert_eq!(back.state_hash(), p.state_hash());
    assert_eq!(back.region(rid).unwrap().1.loop_length, 960);
    assert_eq!(back.markers[&1920], "chorus");
}
