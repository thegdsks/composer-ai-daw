//! Built in voices (D29): the browser engine path equals the offline render bit for bit.

use composer_engine::BuiltinVoice;
use composer_model::{Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{render_project, RenderSettings};
use composer_wasm::{compile_project, decode, encode, BlobError, Session};
use std::collections::BTreeMap;

const SR: u32 = 48_000;

/// One track per voice, each with a short phrase of chords and a held note, so polyphony,
/// release and several velocities are all exercised.
fn project(sounds: &[Option<&str>]) -> Project {
    let mut p = Project::new(Id(1), "voices");
    for (i, sound) in sounds.iter().enumerate() {
        let i = i as u128;
        let notes: BTreeMap<Id, Note> = [
            (0i64, 57u8, 100u8),
            (240, 64, 70),
            (480, 72, 127),
            (480, 36, 90),
        ]
        .iter()
        .enumerate()
        .map(|(n, (start, pitch, velocity))| {
            let id = Id(100 + i * 10 + n as u128);
            (
                id,
                Note {
                    id,
                    pitch: *pitch,
                    velocity: *velocity,
                    start: *start,
                    duration: 720,
                },
            )
        })
        .collect();
        let region = Region {
            loop_length: 0,
            id: Id(50 + i),
            start: 0,
            length: 1920,
            content: RegionContent::Midi { notes },
        };
        p.tracks.push(Track {
            id: Id(10 + i),
            name: format!("t{i}"),
            kind: TrackKind::Instrument,
            instrument: None,
            mix: Default::default(),
            sound: sound.map(str::to_string),
            reverb_send: 0,
            regions: BTreeMap::from([(region.id, region)]),
        });
    }
    p
}

fn wasm_render(p: &Project, blob: &[u8]) -> (Vec<f32>, u64) {
    let _ = p;
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
    (out, s.clipped_frames())
}

#[test]
fn every_voice_matches_the_offline_render_bit_for_bit() {
    for v in BuiltinVoice::ALL {
        let p = project(&[Some(v.sound_id())]);
        let blob = compile_project(&p, SR, 1.0);
        let (wasm, _) = wasm_render(&p, &blob);
        let native = render_project(&p, &RenderSettings::new(SR, 1.0));
        assert!(native.fallbacks.is_empty());
        assert_eq!(native.samples, wasm, "{v:?}");
        assert!(wasm.iter().any(|x| *x != 0.0), "{v:?} silent");
    }
}

#[test]
fn all_voices_together_match_and_blob_round_trips() {
    let ids: Vec<Option<&str>> = BuiltinVoice::ALL
        .iter()
        .map(|v| Some(v.sound_id()))
        .collect();
    let p = project(&ids);
    let blob = compile_project(&p, SR, 1.0);
    // Still blob v2: voices ride in the waveform byte, nothing else changed.
    assert_eq!(&blob[4..8], &2u32.to_le_bytes());
    let d = decode(&blob).unwrap();
    let codes: Vec<u8> = d.tracks.iter().map(|t| t.waveform).collect();
    assert_eq!(codes, (2..=17).collect::<Vec<u8>>());
    assert_eq!(encode(&d), blob);
    let (wasm, clipped) = wasm_render(&p, &blob);
    let native = render_project(&p, &RenderSettings::new(SR, 1.0));
    assert_eq!(native.samples, wasm);
    assert_eq!(native.clipped_frames, clipped);
}

#[test]
fn unknown_and_absent_sounds_keep_the_old_behaviour() {
    let none = project(&[None]);
    let unknown = project(&[Some("freepats-concert-harp")]);
    // A sound id the engine does not generate leaves the track on its instrument slot.
    assert_eq!(
        compile_project(&none, SR, 0.5),
        compile_project(&unknown, SR, 0.5)
    );
    let a = render_project(&none, &RenderSettings::new(SR, 0.5));
    let b = render_project(&unknown, &RenderSettings::new(SR, 0.5));
    assert_eq!(a.samples, b.samples);
}

#[test]
fn waveform_bytes_past_the_voices_are_refused() {
    let p = project(&[Some("builtin-bell")]);
    let mut blob = compile_project(&p, SR, 0.5);
    blob[32] = 18;
    assert_eq!(decode(&blob), Err(BlobError::Waveform));
    blob[32] = 255;
    assert_eq!(decode(&blob), Err(BlobError::Waveform));
}
