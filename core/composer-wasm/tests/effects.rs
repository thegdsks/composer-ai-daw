//! Effect chains in the render and the browser blob (v7, D45): same bits as the offline render at
//! any block size, a round trip, older versions unchanged, hostile sections refused.

use composer_commands::{Command, Editor};
use composer_model::{
    EffectKind, EffectOwner, Id, Note, Project, Region, RegionContent, Track, TrackKind,
};
use composer_render::{render_project, RenderSettings};
use composer_wasm::{compile_project, decode, encode, BlobError, Session};
use std::collections::BTreeMap;

const SR: u32 = 48_000;

fn base() -> Project {
    let mut p = Project::new(Id(1), "fx");
    for k in 0..2u128 {
        let notes: BTreeMap<Id, Note> = (0..40i64)
            .map(|i| {
                let id = Id(1000 + k * 1000 + i as u128);
                (
                    id,
                    Note {
                        id,
                        pitch: 48 + 7 * k as u8 + (i % 5) as u8,
                        velocity: 100,
                        start: i * 960,
                        duration: 700,
                    },
                )
            })
            .collect();
        let region = Region {
            id: Id(20 + k),
            start: 0,
            length: 40 * 960,
            loop_length: 0,
            content: RegionContent::Midi { notes },
        };
        p.tracks.push(Track {
            id: Id(10 + k),
            name: format!("t{k}"),
            kind: TrackKind::Instrument,
            instrument: None,
            mix: Default::default(),
            sound: None,
            reverb_send: 0,
            regions: BTreeMap::from([(region.id, region)]),
        });
    }
    p.reindex();
    p
}

fn with_effects() -> Project {
    let mut ed = Editor::new(base());
    let mut n = 500u128;
    let mut add = |owner, kind| {
        n += 1;
        ed.execute(&Command::AddEffect {
            id: Id(n),
            owner,
            kind,
            index: None,
        })
        .unwrap();
        Id(n)
    };
    let t0 = EffectOwner::Track { track: Id(10) };
    let t1 = EffectOwner::Track { track: Id(11) };
    let gate = add(t0, EffectKind::Gate);
    add(t0, EffectKind::Delay);
    add(t0, EffectKind::Crusher);
    add(t1, EffectKind::Tidal);
    add(t1, EffectKind::Waveshaper);
    add(EffectOwner::Master, EffectKind::StereoTool);
    let bypassed = add(EffectOwner::Master, EffectKind::Delay);
    ed.execute(&Command::SetEffectParam {
        effect: gate,
        param: "threshold".into(),
        value: 20,
    })
    .unwrap();
    ed.execute(&Command::SetEffectBypass {
        effect: bypassed,
        bypass: true,
    })
    .unwrap();
    ed.execute(&Command::SetMasterReverb {
        size: 600,
        mix: 200,
    })
    .unwrap();
    ed.project().clone()
}

fn wasm_render(blob: &[u8], block: u64) -> Vec<f32> {
    let mut s = Session::from_blob(blob).unwrap();
    let mut out = Vec::new();
    let mut left = s.length_frames;
    while left > 0 {
        let n = s.process(left.min(block) as usize);
        for i in 0..n {
            out.push(s.left[i]);
            out.push(s.right[i]);
        }
        left -= n as u64;
    }
    out
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn blob_matches_the_offline_render_bit_for_bit_at_any_block_size() {
    let p = with_effects();
    let blob = compile_project(&p, SR, 2.0);
    assert_eq!(u32::from_le_bytes(blob[4..8].try_into().unwrap()), 7);
    let native = render_project(&p, &RenderSettings::new(SR, 2.0).with_project_mix(&p)).samples;
    let wasm = wasm_render(&blob, 128);
    assert_eq!(bits(&native), bits(&wasm));
    assert_eq!(bits(&wasm), bits(&wasm_render(&blob, 37)));
    // The chains change the sound against the same project without them.
    let plain = render_project(
        &base(),
        &RenderSettings::new(SR, 2.0).with_project_mix(&base()),
    );
    assert_ne!(bits(&plain.samples), bits(&native));
}

#[test]
fn bypassed_chains_render_like_no_chains() {
    let mut ed = Editor::new(base());
    for (i, owner) in [
        EffectOwner::Master,
        EffectOwner::Track { track: Id(10) },
        EffectOwner::Track { track: Id(11) },
    ]
    .into_iter()
    .enumerate()
    {
        ed.execute(&Command::AddEffect {
            id: Id(900 + i as u128),
            owner,
            kind: EffectKind::Waveshaper,
            index: None,
        })
        .unwrap();
        ed.execute(&Command::SetEffectBypass {
            effect: Id(900 + i as u128),
            bypass: true,
        })
        .unwrap();
    }
    let p = ed.project().clone();
    let a = render_project(&p, &RenderSettings::new(SR, 1.0).with_project_mix(&p)).samples;
    let b = render_project(
        &base(),
        &RenderSettings::new(SR, 1.0).with_project_mix(&base()),
    )
    .samples;
    assert_eq!(bits(&a), bits(&b));
}

#[test]
fn blob_round_trips_and_a_project_without_chains_keeps_its_old_version() {
    let p = with_effects();
    let blob = compile_project(&p, SR, 0.5);
    let d = decode(&blob).unwrap();
    assert_eq!(d.effects.len(), 3);
    assert_eq!(encode(&d), blob);
    let plain = compile_project(&base(), SR, 0.5);
    assert_eq!(u32::from_le_bytes(plain[4..8].try_into().unwrap()), 2);
    assert!(decode(&plain).unwrap().effects.is_empty());
}

#[test]
fn hostile_effect_sections_fail_closed() {
    let p = with_effects();
    let blob = compile_project(&p, SR, 0.5);
    let d = decode(&blob).unwrap();
    let bytes: usize = 4 + d
        .effects
        .iter()
        .map(|c| 8 + 20 * c.effects.len())
        .sum::<usize>();
    let tail = blob.len() - bytes;
    for cut in tail..blob.len() {
        assert!(decode(&blob[..cut]).is_err(), "cut {cut}");
    }
    let mut bad = blob.clone();
    bad[tail..tail + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Truncated));
    // Track index past the list.
    let mut bad = blob.clone();
    bad[tail + 4..tail + 8].copy_from_slice(&77u32.to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Effects));
    // Unknown kind, bad bypass byte, nonzero reserved byte.
    for (at, v) in [(0usize, 99u8), (1, 7), (3, 1)] {
        let mut bad = blob.clone();
        bad[tail + 12 + at] = v;
        assert_eq!(decode(&bad), Err(BlobError::Effects), "byte {at}");
    }
    // Parameter out of range.
    let mut bad = blob.clone();
    bad[tail + 12 + 4..tail + 12 + 6].copy_from_slice(&i16::MAX.to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Effects));
}
