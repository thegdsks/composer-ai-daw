//! Sample banks in the browser blob (D34): v3 round trip, parity with the offline render,
//! and hostile blobs.

use composer_engine::{
    Adsr, BankParams, InstrumentDef, LoopMode, SampleBank, SampleData, Zone, BUILTIN_SINE,
};
use composer_model::{Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{render_project, RenderSettings};
use composer_wasm::{compile_project, compile_project_with_banks, decode, BlobError, Session};
use std::collections::BTreeMap;
use std::sync::Arc;

const SR: u32 = 48_000;
const BANK_ID: Id = Id(900);

fn bank() -> Arc<SampleBank> {
    let mono: Vec<f32> = (0..3000)
        .map(|i| ((i * 13) % 97) as f32 / 97.0 - 0.5)
        .collect();
    let stereo: Vec<f32> = (0..4000)
        .map(|i| ((i * 5) % 61) as f32 / 61.0 - 0.5)
        .collect();
    let zone = |sample, lo, hi, vlo, vhi, mode, pan| Zone {
        sample,
        key_lo: lo,
        key_hi: hi,
        vel_lo: vlo,
        vel_hi: vhi,
        root_key: 64,
        tune_cents: 7,
        scale_cents: 100,
        loop_mode: mode,
        loop_start: 300,
        loop_end: 1200,
        adsr: Adsr {
            attack: 0.002,
            decay: 0.05,
            sustain: 0.6,
            release: 0.03,
        },
        gain: 0.4,
        pan,
    };
    Arc::new(
        SampleBank::new(
            vec![
                SampleData::new(44_100, 1, 0, mono).unwrap(),
                SampleData::new(32_000, 2, 0, stereo).unwrap(),
            ],
            vec![
                zone(0, 0, 70, 0, 80, LoopMode::Continuous, -0.7),
                zone(0, 0, 70, 0, 80, LoopMode::Sustain, 0.7),
                zone(1, 0, 127, 81, 127, LoopMode::OneShot, 0.0),
                zone(1, 71, 127, 0, 80, LoopMode::None, 0.2),
            ],
        )
        .unwrap(),
    )
}

fn project() -> Project {
    let mut p = Project::new(Id(1), "bank");
    let notes: BTreeMap<Id, Note> = [
        (55u8, 60u8, 0i64),
        (62, 110, 480),
        (69, 90, 960),
        (60, 40, 1440),
    ]
    .iter()
    .enumerate()
    .map(|(i, (pitch, velocity, start))| {
        let id = Id(i as u128 + 10);
        (
            id,
            Note {
                id,
                pitch: *pitch,
                velocity: *velocity,
                start: *start,
                duration: 700,
            },
        )
    })
    .collect();
    let region = Region {
        loop_length: 0,
        id: Id(2),
        start: 0,
        length: 3840,
        content: RegionContent::Midi { notes },
    };
    p.tracks.push(Track {
        id: Id(3),
        name: "sampler".into(),
        kind: TrackKind::Instrument,
        instrument: Some(BANK_ID),
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    });
    p
}

fn banks() -> BTreeMap<Id, Arc<SampleBank>> {
    BTreeMap::from([(BANK_ID, bank())])
}

#[test]
fn wasm_bank_path_equals_offline_render_bit_for_bit() {
    let p = project();
    let blob = compile_project_with_banks(&p, SR, 0.5, &banks());
    let mut s = Session::from_blob(&blob).unwrap();
    let mut wasm = Vec::new();
    let mut left = s.length_frames;
    while left > 0 {
        let n = s.process(left.min(128) as usize);
        for i in 0..n {
            wasm.push(s.left[i]);
            wasm.push(s.right[i]);
        }
        left -= n as u64;
    }
    let mut settings = RenderSettings::new(SR, 0.5).with_project_mix(&p);
    settings.instruments.insert(
        BANK_ID,
        InstrumentDef::Bank(BankParams {
            bank: bank(),
            gain: 1.0,
        }),
    );
    let native = render_project(&p, &settings);
    assert!(native.fallbacks.is_empty());
    assert_eq!(native.samples, wasm);
    assert!(wasm.iter().any(|v| *v != 0.0));
}

#[test]
fn v3_round_trips_and_default_projects_stay_v2() {
    let p = project();
    let blob = compile_project_with_banks(&p, SR, 0.5, &banks());
    assert_eq!(&blob[4..8], &3u32.to_le_bytes());
    let d = decode(&blob).unwrap();
    assert_eq!(d.banks.len(), 1);
    assert_eq!(*d.banks[0], *bank());
    assert_eq!(d.tracks[0].bank, Some(0));
    assert_eq!(composer_wasm::encode(&d), blob);
    // No bank map, or a map the project does not use: same bytes as before this change.
    let plain = compile_project(&p, SR, 0.5);
    assert_eq!(&plain[4..8], &2u32.to_le_bytes());
    let mut q = p.clone();
    q.tracks[0].instrument = Some(BUILTIN_SINE);
    assert_eq!(
        compile_project_with_banks(&q, SR, 0.5, &banks()),
        compile_project(&q, SR, 0.5)
    );
}

#[test]
fn every_prefix_and_corruption_of_a_v3_blob_is_safe() {
    let blob = compile_project_with_banks(&project(), SR, 0.1, &banks());
    for n in 0..blob.len() {
        assert!(decode(&blob[..n]).is_err(), "prefix {n}");
    }
    // Flip single bytes across the header, tracks, events and the bank section.
    let mut seed = 99u32;
    for i in (0..blob.len())
        .step_by(3)
        .chain(blob.len() - 400..blob.len())
    {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut b = blob.clone();
        b[i] = (seed >> 24) as u8;
        if let Ok(d) = decode(&b).and_then(|d| {
            if d.voices_per_track <= 64 {
                Ok(d)
            } else {
                Err(BlobError::Bank)
            }
        }) {
            // Whatever decodes must also play without panicking.
            let mut s = Session::new(d);
            s.process(128);
        }
    }
}

#[test]
fn huge_counts_fail_before_allocating() {
    let blob = compile_project_with_banks(&project(), SR, 0.1, &banks());
    let bank_at = 32 + 16 + 8 * 16;
    let mut b = blob.clone();
    // bank_count, sample_count and a frame count all claiming far more than the blob holds.
    b[bank_at..bank_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode(&b), Err(BlobError::Bank));
    let mut b = blob.clone();
    b[bank_at + 4..bank_at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode(&b), Err(BlobError::Bank));
    let mut b = blob.clone();
    b[bank_at + 12 + 8..bank_at + 12 + 12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode(&b).is_err());
    // A track pointing at a bank that is not in the blob.
    let mut b = blob;
    b[32 + 14..32 + 16].copy_from_slice(&5u16.to_le_bytes());
    assert_eq!(decode(&b), Err(BlobError::Bank));
}
