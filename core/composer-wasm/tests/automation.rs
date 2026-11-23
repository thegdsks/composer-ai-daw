//! Automation in the browser blob (v6, D36): the same sound as the offline render, whatever the
//! block size, and every older blob version still decodes.

use composer_commands::{Command, Editor};
use composer_model::{
    AutomationCurve, AutomationTarget, Id, Note, Project, Region, RegionContent, Track, TrackKind,
};
use composer_render::{render_project, RenderSettings};
use composer_wasm::{compile_project, decode, encode, BlobError, Session};
use std::collections::BTreeMap;

const SR: u32 = 48_000;

/// Two tracks playing a note every beat for two minutes (240 beats at 120 bpm).
fn base() -> Project {
    let mut p = Project::new(Id(1), "fade");
    for k in 0..2u128 {
        let notes: BTreeMap<Id, Note> = (0..240i64)
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
            length: 240 * 960,
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

fn with_lanes(p: Project) -> Project {
    let mut ed = Editor::new(p);
    let add = |t: AutomationTarget, tick: i64, value: i16, curve| Command::AddAutomationPoint {
        target: t,
        tick,
        value,
        curve: Some(curve),
    };
    let vol = AutomationTarget::TrackVolume { track: Id(10) };
    let pan = AutomationTarget::TrackPan { track: Id(11) };
    let send = AutomationTarget::TrackReverbSend { track: Id(10) };
    let master = AutomationTarget::MasterVolume;
    for c in [
        // A two minute fade out of the first track.
        add(vol, 0, 1000, AutomationCurve::Linear),
        add(vol, 240 * 960, 0, AutomationCurve::Linear),
        add(pan, 0, -1000, AutomationCurve::Smooth),
        add(pan, 100 * 960, 1000, AutomationCurve::Hold),
        add(pan, 150 * 960, -300, AutomationCurve::Linear),
        add(send, 10 * 960, 0, AutomationCurve::Smooth),
        add(send, 200 * 960, 900, AutomationCurve::Linear),
        add(master, 0, 1400, AutomationCurve::Smooth),
        add(master, 120 * 960, 600, AutomationCurve::Linear),
    ] {
        ed.execute(&c).unwrap();
    }
    ed.execute(&Command::SetMasterReverb {
        size: 600,
        mix: 300,
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

#[test]
fn two_minute_fade_matches_the_render_bit_for_bit_at_any_block_size() {
    let p = with_lanes(base());
    let blob = compile_project(&p, SR, 1.0);
    assert_eq!(u32::from_le_bytes(blob[4..8].try_into().unwrap()), 6);
    let native = render_project(&p, &RenderSettings::new(SR, 1.0).with_project_mix(&p)).samples;
    assert!(native.len() > 2 * 120 * SR as usize);
    let wasm = wasm_render(&blob, 128);
    assert_eq!(native.len(), wasm.len());
    assert!(
        native
            .iter()
            .zip(&wasm)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "wasm differs from native"
    );
    // The host block size must not change the result.
    assert_eq!(wasm, wasm_render(&blob, 37));
    // The fade is audible: the first track is louder early than late.
    let level =
        |from: usize, to: usize| native[from..to].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let early = level(2 * 5 * SR as usize, 2 * 15 * SR as usize);
    let late = level(2 * 110 * SR as usize, 2 * 119 * SR as usize);
    assert!(late < early, "{late} {early}");
    // And automation really changes the sound against the same project without lanes.
    let plain = render_project(
        &base(),
        &RenderSettings::new(SR, 1.0).with_project_mix(&base()),
    );
    assert_ne!(plain.samples, native);
}

#[test]
fn blob_round_trips_and_a_project_without_lanes_keeps_its_old_version() {
    let p = with_lanes(base());
    let blob = compile_project(&p, SR, 0.5);
    let d = decode(&blob).unwrap();
    assert_eq!(d.lanes.len(), 4);
    assert_eq!(encode(&d), blob);
    let plain = compile_project(&base(), SR, 0.5);
    assert_eq!(u32::from_le_bytes(plain[4..8].try_into().unwrap()), 2);
    assert!(decode(&plain).unwrap().lanes.is_empty());
}

#[test]
fn hostile_lane_sections_fail_closed() {
    let p = with_lanes(base());
    let blob = compile_project(&p, SR, 0.5);
    let d = decode(&blob).unwrap();
    // The lane section is the tail of the blob: cut it anywhere and decoding must not panic.
    let tail = blob.len() - encode_lane_bytes(&d);
    for cut in (tail..blob.len()).step_by(7) {
        assert!(decode(&blob[..cut]).is_err(), "cut {cut}");
    }
    // A huge lane count is refused before any allocation.
    let mut bad = blob.clone();
    bad[tail..tail + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Truncated));
    // A track index past the track list.
    let mut bad = blob.clone();
    bad[tail + 4..tail + 8].copy_from_slice(&77u32.to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Automation));
    // An unknown curve code.
    let mut bad = blob.clone();
    bad[tail + 4 + 16 + 10] = 9;
    assert_eq!(decode(&bad), Err(BlobError::Automation));
}

/// Size of the automation section of a decoded blob.
fn encode_lane_bytes(d: &composer_wasm::Decoded) -> usize {
    4 + d
        .lanes
        .iter()
        .map(|l| 16 + 16 * l.points.len())
        .sum::<usize>()
}
