use composer_format::{
    read_project, write_project, AssetRef, AssetTable, ContentHash, Limits, Meta,
};
use composer_model::{Change, Id, Project, Region, RegionContent, Track, TrackKind};

fn id(n: u128) -> Id {
    Id(n)
}

fn meta() -> Meta {
    Meta {
        app_version: "0.0.1".into(),
        created_ms: 1,
        modified_ms: 2,
        transport: None,
    }
}

fn project_with_clip(hash: &ContentHash) -> Project {
    let mut p = Project::new(id(1), "clips");
    let clip = Region {
        loop_length: 0,
        id: id(0x60),
        start: 0,
        length: 960,
        content: RegionContent::Clip {
            asset: hash.to_hex(),
            offset: 0,
            gain: 1000,
        },
    };
    let track = Track {
        id: id(0x12),
        name: "voice".into(),
        kind: TrackKind::Audio,
        instrument: None,
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: [(clip.id, clip)].into_iter().collect(),
    };
    p.apply(&Change::NewTrack { index: 0, track }).unwrap();
    p
}

#[test]
fn clip_must_name_an_asset_in_the_table() {
    let hash = ContentHash::of(b"voice.wav");
    let p = project_with_clip(&hash);
    let none = write_project(&p, &AssetTable::new(), &meta());
    assert_eq!(
        read_project(&none, &Limits::default()).unwrap_err().code(),
        "dangling_reference"
    );
    let mut t = AssetTable::new();
    t.insert(AssetRef {
        id: id(0xa9),
        hash,
        bytes: 10,
        name: "voice.wav".into(),
        sample_rate: 48_000,
        channels: 1,
    })
    .unwrap();
    let ok = write_project(&p, &t, &meta());
    let l = read_project(&ok, &Limits::default()).unwrap();
    assert_eq!(l.project.state_hash(), p.state_hash());
    assert_eq!(l.assets, t);
}

#[test]
fn hash_hex_round_trips_and_rejects_junk() {
    let h = ContentHash::of(b"x");
    assert_eq!(ContentHash::from_hex(&h.to_hex()), Some(h));
    assert_eq!(ContentHash::from_hex("XYZ"), None);
    assert_eq!(ContentHash::from_hex(&"A".repeat(64)), None);
}

#[test]
fn malformed_clip_fields_are_refused_on_read() {
    let hash = ContentHash::of(b"a");
    let mut p = project_with_clip(&hash);
    if let Some(r) = p.tracks[0].regions.values_mut().next() {
        r.content = RegionContent::Clip {
            asset: "NOT A HASH".into(),
            offset: 0,
            gain: 1000,
        };
    }
    let b = write_project(&p, &AssetTable::new(), &meta());
    assert_eq!(
        read_project(&b, &Limits::default()).unwrap_err().code(),
        "out_of_range"
    );
}
