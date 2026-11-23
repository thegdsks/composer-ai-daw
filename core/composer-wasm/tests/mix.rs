//! Browser engine path versus offline render with a non default channel mix (D19).

use composer_engine::MasterBus;
use composer_model::{ChannelMix, Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{render_project, RenderSettings};
use composer_wasm::{
    compile_project, compile_project_with_bus, decode, encode, BlobError, Session,
};
use std::collections::BTreeMap;

const SR: u32 = 48_000;

fn track(id: u128, pitch: u8, mix: ChannelMix) -> Track {
    let nid = Id(id * 10);
    let note = Note {
        id: nid,
        pitch,
        velocity: 100,
        start: 0,
        duration: 960,
    };
    let region = Region {
        loop_length: 0,
        id: Id(id + 1000),
        start: 0,
        length: 1920,
        content: RegionContent::Midi {
            notes: BTreeMap::from([(nid, note)]),
        },
    };
    Track {
        id: Id(id),
        name: format!("t{id}"),
        kind: TrackKind::Instrument,
        instrument: None,
        mix,
        sound: None,
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    }
}

fn mix(volume: u16, pan: i16, mute: bool, solo: bool) -> ChannelMix {
    ChannelMix {
        volume,
        pan,
        mute,
        solo,
    }
}

fn project(mixes: &[ChannelMix]) -> Project {
    let mut p = Project::new(Id(1), "mix");
    for (i, m) in mixes.iter().enumerate() {
        p.tracks.push(track(10 + i as u128, 57 + 7 * i as u8, *m));
    }
    p
}

fn wasm_render(p: &Project) -> Vec<f32> {
    let mut s = Session::from_blob(&compile_project(p, SR, 0.25)).unwrap();
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

fn native_render(p: &Project) -> Vec<f32> {
    render_project(p, &RenderSettings::new(SR, 0.25).with_project_mix(p)).samples
}

#[test]
fn wasm_path_equals_native_render_bit_for_bit() {
    let cases = [
        vec![mix(1000, 0, false, false), mix(1000, 0, false, false)],
        vec![mix(500, -700, false, false), mix(1500, 1000, false, false)],
        vec![mix(800, 300, true, false), mix(1000, -300, false, false)],
        vec![mix(1000, 0, false, true), mix(700, 400, false, false)],
        vec![
            mix(1000, 0, false, true),
            mix(1000, 0, false, true),
            mix(900, 0, true, true),
        ],
    ];
    for c in cases {
        let p = project(&c);
        let (a, b) = (native_render(&p), wasm_render(&p));
        assert_eq!(a, b, "mix {c:?}");
        assert!(a.iter().any(|s| *s != 0.0));
    }
}

fn peak(samples: &[f32], ch: usize) -> f32 {
    samples
        .iter()
        .skip(ch)
        .step_by(2)
        .fold(0.0, |m, s| m.max(s.abs()))
}

#[test]
fn mute_solo_and_pan_law() {
    let base = wasm_render(&project(&[mix(1000, 0, false, false)]));
    let muted = wasm_render(&project(&[mix(1000, 0, true, false)]));
    assert!(muted.iter().all(|s| *s == 0.0));
    // A soloed track silences the others: same output as the soloed track alone
    // (the other track is placed second so its pitch differs from the first).
    let solo_second = wasm_render(&project(&[
        mix(1000, 0, false, false),
        mix(1000, 0, false, true),
    ]));
    let second_only = wasm_render(&project(&[
        mix(1000, 0, true, false),
        mix(1000, 0, false, false),
    ]));
    assert_eq!(solo_second, second_only);
    // Constant power: center is about -3 dB per side, hard pans sound on one side only.
    let l = wasm_render(&project(&[mix(1000, -1000, false, false)]));
    let r = wasm_render(&project(&[mix(1000, 1000, false, false)]));
    assert_eq!(peak(&l, 1), 0.0);
    assert_eq!(peak(&r, 0), 0.0);
    let ratio = peak(&base, 0) / peak(&l, 0);
    assert!(
        (ratio - core::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3,
        "{ratio}"
    );
    assert_eq!(peak(&l, 0), peak(&r, 1));
}

#[test]
fn v1_blob_still_reads_and_plays_with_default_mix() {
    let p = project(&[mix(1000, 0, false, false)]);
    let v2 = compile_project(&p, SR, 0.25);
    let d = decode(&v2).unwrap();
    // Hand built v1: version 1, 12 byte track records.
    let mut v1 = v2[..32].to_vec();
    v1[4..8].copy_from_slice(&1u32.to_le_bytes());
    for t in &d.tracks {
        v1.extend_from_slice(&[t.waveform, 0, 0, 0]);
        v1.extend_from_slice(&t.gain.to_bits().to_le_bytes());
        v1.extend_from_slice(&t.pan.to_bits().to_le_bytes());
    }
    v1.extend_from_slice(&v2[32 + 16 * d.tracks.len()..]);
    let d1 = decode(&v1).unwrap();
    assert_eq!(d1, d);
    assert_eq!(d1.tracks[0].mix, ChannelMix::default());
    assert_eq!(encode(&d1), v2);
    // Unknown version and out of range mix fail closed.
    let mut bad = v2.clone();
    bad[4] = 9;
    assert_eq!(decode(&bad), Err(BlobError::Version));
    let mut bad = v2;
    bad[32 + 2..32 + 4].copy_from_slice(&5000u16.to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Mix));
}

fn bus() -> (MasterBus, BTreeMap<Id, f32>) {
    (
        MasterBus::new(0.7, 0.8, 0.5),
        BTreeMap::from([(Id(10), 0.6), (Id(11), 1.0)]),
    )
}

fn wasm_bus_render(p: &Project, master: MasterBus, sends: &BTreeMap<Id, f32>) -> Vec<f32> {
    let blob = compile_project_with_bus(p, SR, 0.5, &BTreeMap::new(), master, sends);
    let mut s = Session::from_blob(&blob).unwrap();
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
fn master_and_reverb_wasm_equals_native_bit_for_bit() {
    let p = project(&[mix(900, -300, false, false), mix(1000, 400, false, false)]);
    let (master, sends) = bus();
    let mut settings = RenderSettings::new(SR, 0.5).with_project_mix(&p);
    settings.master = master;
    settings.sends = sends.clone();
    let native = render_project(&p, &settings).samples;
    let wasm = wasm_bus_render(&p, master, &sends);
    assert_eq!(native, wasm);
    // The reverb adds energy after the dry notes end (the dry tail is 0.5 s of release at most).
    let dry = native_render(&p);
    assert_ne!(dry.len(), 0);
    let wet_only_tail = native.iter().skip(native.len() - 2000).any(|s| *s != 0.0);
    assert!(wet_only_tail);
}

#[test]
fn master_volume_scales_and_default_is_untouched() {
    let p = project(&[mix(1000, 0, false, false)]);
    let none = BTreeMap::new();
    let base = wasm_bus_render(&p, MasterBus::default(), &none);
    assert_eq!(base, wasm_render_tail(&p, 0.5));
    let half = wasm_bus_render(&p, MasterBus::new(0.5, 0.5, 0.0), &none);
    for (a, b) in base.iter().zip(&half) {
        assert_eq!(a * 0.5, *b);
    }
    // A send does nothing while the reverb return is off.
    let sends = BTreeMap::from([(Id(10), 1.0)]);
    assert_eq!(wasm_bus_render(&p, MasterBus::default(), &sends), base);
    // The default bus keeps the plain v2 blob.
    assert_eq!(
        compile_project_with_bus(&p, SR, 0.5, &BTreeMap::new(), MasterBus::default(), &none),
        compile_project(&p, SR, 0.5)
    );
}

fn wasm_render_tail(p: &Project, tail: f64) -> Vec<f32> {
    let mut s = Session::from_blob(&compile_project(p, SR, tail)).unwrap();
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
fn v4_roundtrip_and_fail_closed() {
    let p = project(&[mix(1000, 0, false, false), mix(1000, 0, false, false)]);
    let (master, sends) = bus();
    let blob = compile_project_with_bus(&p, SR, 0.5, &BTreeMap::new(), master, &sends);
    let d = decode(&blob).unwrap();
    assert_eq!(d.master, master);
    assert_eq!(d.tracks[0].send, 0.6);
    assert_eq!(encode(&d), blob);
    let master_at = 32 + 16 * d.tracks.len() + 16 * d.events.len();
    let mut bad = blob.clone();
    bad[master_at..master_at + 4].copy_from_slice(&5.0f32.to_bits().to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Master));
    let mut bad = blob.clone();
    bad[master_at + 16..master_at + 20].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    assert_eq!(decode(&bad), Err(BlobError::Master));
    assert_eq!(decode(&blob[..master_at + 10]), Err(BlobError::Truncated));
}

#[test]
fn project_master_and_send_sound_the_same_in_wasm_and_native() {
    use composer_model::{Change, FieldKey, FieldValue, MasterMix};
    let mut p = project(&[mix(900, -300, false, false), mix(1000, 400, false, false)]);
    p.reindex();
    let dry = wasm_render_tail(&p, 0.5);
    p.apply(&Change::Set {
        key: FieldKey::Master,
        old: FieldValue::Master(p.master),
        new: FieldValue::Master(MasterMix {
            volume: 800,
            reverb_size: 700,
            reverb_mix: 400,
        }),
    })
    .unwrap();
    p.apply(&Change::Set {
        key: FieldKey::TrackReverbSend { track: Id(10) },
        old: FieldValue::Permille(0),
        new: FieldValue::Permille(600),
    })
    .unwrap();
    let wasm = wasm_render_tail(&p, 0.5);
    let native = render_project(&p, &RenderSettings::new(SR, 0.5).with_project_mix(&p)).samples;
    assert_eq!(native, wasm);
    assert_ne!(wasm, dry);
    // The model values select blob v4.
    let blob = compile_project(&p, SR, 0.5);
    assert_eq!(u32::from_le_bytes(blob[4..8].try_into().unwrap()), 4);
}
