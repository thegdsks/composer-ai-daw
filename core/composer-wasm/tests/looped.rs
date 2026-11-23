//! Looped clips in the browser blob (v5, D24): same sound as the offline render, and every
//! older blob version still decodes.

use composer_model::{Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{render_project, RenderSettings};
use composer_wasm::{compile_project, decode, encode, BlobError, Session};
use std::collections::BTreeMap;

const SR: u32 = 48_000;

fn project(loop_length: i64) -> Project {
    let notes: BTreeMap<Id, Note> = [(0, 60u8), (960, 64), (1500, 67)]
        .into_iter()
        .enumerate()
        .map(|(i, (start, pitch))| {
            let id = Id(100 + i as u128);
            (
                id,
                Note {
                    id,
                    pitch,
                    velocity: 100,
                    start,
                    duration: 400,
                },
            )
        })
        .collect();
    let region = Region {
        id: Id(11),
        start: 480,
        length: 7000,
        loop_length,
        content: RegionContent::Midi { notes },
    };
    let mut p = Project::new(Id(1), "loop");
    p.tracks.push(Track {
        id: Id(10),
        name: "keys".into(),
        kind: TrackKind::Instrument,
        instrument: None,
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    });
    p
}

fn wasm_render(blob: &[u8]) -> Vec<f32> {
    let mut s = Session::from_blob(blob).unwrap();
    let mut out = Vec::new();
    let mut left = s.length_frames;
    while left > 0 {
        let n = s.process(left.min(128) as usize);
        for i in 0..n {
            out.push(s.left[i]);
            out.push(s.right[i]);
        }
        left -= n as u64;
    }
    out
}

#[test]
fn looped_blob_is_v5_carries_the_loop_and_matches_the_render() {
    let p = project(2000);
    let blob = compile_project(&p, SR, 0.25);
    assert_eq!(u32::from_le_bytes(blob[4..8].try_into().unwrap()), 5);
    let d = decode(&blob).unwrap();
    assert_eq!(d.loops.len(), 1);
    // 2000 ticks at 120 bpm and 960 ticks per quarter.
    assert_eq!(d.loops[0].period_frames, 2000 * u64::from(SR) / 1920);
    assert_eq!(encode(&d), blob);
    let native = render_project(&p, &RenderSettings::new(SR, 0.25).with_project_mix(&p)).samples;
    assert_eq!(native, wasm_render(&blob));
    assert!(native.iter().any(|s| *s != 0.0));
}

#[test]
fn unlooped_project_keeps_its_old_blob_version() {
    let blob = compile_project(&project(0), SR, 0.25);
    assert_eq!(u32::from_le_bytes(blob[4..8].try_into().unwrap()), 2);
    assert!(decode(&blob).unwrap().loops.is_empty());
}

#[test]
fn bad_loop_sections_fail_closed() {
    let p = project(2000);
    let mut blob = compile_project(&p, SR, 0.25);
    let n = blob.len();
    // Corrupt the loop track index (first u32 after the count, 28 bytes from the end).
    blob[n - 28..n - 24].copy_from_slice(&9u32.to_le_bytes());
    assert_eq!(decode(&blob), Err(BlobError::Loop));
    // A count larger than the bytes present.
    let mut blob = compile_project(&p, SR, 0.25);
    let n = blob.len();
    blob[n - 32..n - 28].copy_from_slice(&1000u32.to_le_bytes());
    assert_eq!(decode(&blob), Err(BlobError::Truncated));
    // Cut short.
    let blob = compile_project(&p, SR, 0.25);
    assert!(decode(&blob[..blob.len() - 5]).is_err());
}
