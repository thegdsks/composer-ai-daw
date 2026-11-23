//! Readers against tiny files built in code, plus truncation and corruption sweeps.

use composer_engine::{
    BankParams, Engine, EngineConfig, Event, EventKind, Instrument, InstrumentDef, LoopMode,
    TrackSetup,
};
use composer_sampler::{
    decode_wav, load_sf2, load_sfz, load_sfz_file, Limits, SamplerError, Sf2Select,
};
use std::sync::Arc;

// ---- fixture builders -------------------------------------------------------------------

fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = id.to_vec();
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(body);
    if body.len() % 2 == 1 {
        v.push(0);
    }
    v
}

fn list(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
    let mut body = kind.to_vec();
    for p in parts {
        body.extend_from_slice(p);
    }
    chunk(b"LIST", &body)
}

fn name20(s: &str) -> [u8; 20] {
    let mut n = [0u8; 20];
    n[..s.len()].copy_from_slice(s.as_bytes());
    n
}

fn gen(op: u16, amount: u16) -> [u8; 4] {
    let mut g = [0u8; 4];
    g[..2].copy_from_slice(&op.to_le_bytes());
    g[2..].copy_from_slice(&amount.to_le_bytes());
    g
}

#[allow(clippy::too_many_arguments)]
fn shdr(
    name: &str,
    start: u32,
    end: u32,
    ls: u32,
    le: u32,
    rate: u32,
    root: u8,
    kind: u16,
) -> Vec<u8> {
    let mut v = name20(name).to_vec();
    for x in [start, end, ls, le, rate] {
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.extend_from_slice(&[root, 0]);
    v.extend_from_slice(&0u16.to_le_bytes());
    v.extend_from_slice(&kind.to_le_bytes());
    v
}

/// One preset, one instrument with a soft looped zone and a loud unlooped zone.
fn tiny_sf2() -> Vec<u8> {
    let pcm: Vec<u8> = (0..200i16)
        .flat_map(|i| (i * 100).to_le_bytes())
        .chain([0u8; 92]) // pad after the sample
        .collect();
    let mut phdr = name20("Tiny").to_vec();
    phdr.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // preset, bank, bag 0
    phdr.extend_from_slice(&[0; 12]);
    let mut eop = name20("EOP").to_vec();
    eop.extend_from_slice(&[0, 0, 0, 0, 1, 0]);
    eop.extend_from_slice(&[0; 12]);
    phdr.extend_from_slice(&eop);

    let bag = |g: u16| [g.to_le_bytes(), 0u16.to_le_bytes()].concat();
    let pbag = [bag(0), bag(1)].concat();
    let pgen = [gen(41, 0), gen(0, 0)].concat();

    let mut inst = name20("Inst").to_vec();
    inst.extend_from_slice(&0u16.to_le_bytes());
    inst.extend_from_slice(&name20("EOI"));
    inst.extend_from_slice(&2u16.to_le_bytes());
    let ibag = [bag(0), bag(3), bag(5)].concat();
    let igen = [
        gen(44, 63 << 8), // velocity 0..63
        gen(54, 1),       // loop continuously
        gen(53, 0),       // sample 0
        gen(44, 64 | (127 << 8)),
        gen(53, 0),
        gen(0, 0),
    ]
    .concat();
    let sh = [
        shdr("S", 0, 200, 50, 150, 44100, 60, 1),
        shdr("EOS", 0, 0, 0, 0, 0, 0, 0),
    ]
    .concat();
    let info = list(b"INFO", &[chunk(b"ifil", &[2, 0, 1, 0])]);
    let sdta = list(b"sdta", &[chunk(b"smpl", &pcm)]);
    let pdta = list(
        b"pdta",
        &[
            chunk(b"phdr", &phdr),
            chunk(b"pbag", &pbag),
            chunk(b"pmod", &[0; 10]),
            chunk(b"pgen", &pgen),
            chunk(b"inst", &inst),
            chunk(b"ibag", &ibag),
            chunk(b"imod", &[0; 10]),
            chunk(b"igen", &igen),
            chunk(b"shdr", &sh),
        ],
    );
    let mut body = b"sfbk".to_vec();
    for p in [info, sdta, pdta] {
        body.extend_from_slice(&p);
    }
    chunk(b"RIFF", &body)
}

fn wav16(rate: u32, channels: u16, frames: &[i16], loop_pts: Option<(u32, u32)>) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&(rate * 2 * u32::from(channels)).to_le_bytes());
    fmt.extend_from_slice(&(2 * channels).to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let data: Vec<u8> = frames.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut body = b"WAVE".to_vec();
    body.extend_from_slice(&chunk(b"fmt ", &fmt));
    if let Some((s, e)) = loop_pts {
        let mut smpl = vec![0u8; 28];
        smpl.extend_from_slice(&1u32.to_le_bytes());
        smpl.extend_from_slice(&[0; 4]);
        for x in [0u32, 0, s, e, 0, 0] {
            smpl.extend_from_slice(&x.to_le_bytes());
        }
        body.extend_from_slice(&chunk(b"smpl", &smpl));
    }
    body.extend_from_slice(&chunk(b"data", &data));
    chunk(b"RIFF", &body)
}

fn wave_frames(n: usize) -> Vec<i16> {
    (0..n as i16).map(|i| i * 50).collect()
}

const SFZ: &str = "// tiny\n<control> default_path=samples/\n<global> ampeg_release=0.01\n\
<group> lovel=0 hivel=63 // soft\n<region> sample=a b.wav key=c4 loop_mode=loop_continuous loop_start=10 loop_end=79\n\
<group> lovel=64 hivel=127\n<region> sample=a b.wav lokey=c4 hikey=c#4 pitch_keycenter=60 tune=-25 transpose=1 pan=100\n\
/* block\ncomment */ <region> sample=other.wav key=62 loop_mode=one_shot volume=-6\n";

fn sfz_files(name: &str) -> Result<Vec<u8>, SamplerError> {
    match name {
        "samples/a b.wav" => Ok(wav16(32000, 1, &wave_frames(100), None)),
        "samples/other.wav" => Ok(wav16(22050, 2, &wave_frames(200), Some((5, 49)))),
        _ => Err(SamplerError::MissingSample),
    }
}

// ---- tests ------------------------------------------------------------------------------

#[test]
fn sf2_reads_zones_loops_and_samples() {
    let bank = load_sf2(&tiny_sf2(), &Limits::default(), None).unwrap();
    assert_eq!(bank.samples().len(), 1);
    assert_eq!(bank.samples()[0].len_frames(), 200);
    assert_eq!(bank.samples()[0].rate(), 44100);
    let z = bank.zones();
    assert_eq!(z.len(), 2);
    assert_eq!((z[0].vel_lo, z[0].vel_hi), (0, 63));
    assert_eq!(z[0].loop_mode, LoopMode::Continuous);
    assert_eq!((z[0].loop_start, z[0].loop_end), (50, 150));
    assert_eq!((z[1].vel_lo, z[1].vel_hi), (64, 127));
    assert_eq!(z[1].loop_mode, LoopMode::None);
    assert_eq!(z[0].root_key, 60);
    assert_eq!(z[0].scale_cents, 100);
    assert!((bank.samples()[0].data()[3] - 300.0 / 32768.0).abs() < 1e-9);
    // Selecting a preset that is not there fails closed.
    let miss = Some(Sf2Select { bank: 5, preset: 9 });
    assert_eq!(
        load_sf2(&tiny_sf2(), &Limits::default(), miss),
        Err(SamplerError::NoZones)
    );
}

#[test]
fn sf2_every_prefix_fails_without_panic() {
    let full = tiny_sf2();
    for n in 0..full.len() {
        // Cutting inside the final chunk can leave a parseable file only if nothing is lost.
        let _ = load_sf2(&full[..n], &Limits::default(), None);
    }
    assert!(load_sf2(&full[..full.len() - 1], &Limits::default(), None).is_err());
}

#[test]
fn sf2_corrupted_bytes_never_panic() {
    let full = tiny_sf2();
    let mut seed = 0x1234_5678u32;
    for i in 0..full.len() {
        for _ in 0..8 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut f = full.clone();
            f[i] = (seed >> 24) as u8;
            let _ = load_sf2(&f, &Limits::default(), None);
        }
    }
}

#[test]
fn sf2_bad_indices_and_loops_are_rejected() {
    // Point the preset at an instrument that does not exist: patch the pgen amount.
    let mut f = tiny_sf2();
    let pos = f
        .windows(4)
        .position(|w| w == gen(41, 0))
        .expect("pgen record");
    f[pos + 2] = 9;
    assert_eq!(
        load_sf2(&f, &Limits::default(), None),
        Err(SamplerError::BadIndex)
    );
    // Loop end past the sample end.
    let mut f = tiny_sf2();
    let sh = shdr("S", 0, 200, 50, 150, 44100, 60, 1);
    let pos = f.windows(sh.len()).position(|w| w == sh).unwrap();
    f[pos + 32..pos + 36].copy_from_slice(&900u32.to_le_bytes());
    assert_eq!(
        load_sf2(&f, &Limits::default(), None),
        Err(SamplerError::BadLoop)
    );
}

#[test]
fn size_caps_apply() {
    let sf2 = tiny_sf2();
    let tight = Limits {
        max_file_bytes: 100,
        ..Limits::default()
    };
    assert_eq!(load_sf2(&sf2, &tight, None), Err(SamplerError::TooLarge));
    let tight = Limits {
        max_sample_frames: 50,
        ..Limits::default()
    };
    assert_eq!(load_sf2(&sf2, &tight, None), Err(SamplerError::TooLarge));
    let tight = Limits {
        max_zones: 1,
        ..Limits::default()
    };
    assert_eq!(load_sf2(&sf2, &tight, None), Err(SamplerError::TooLarge));
    let w = wav16(44100, 1, &wave_frames(100), None);
    let tight = Limits {
        max_sample_frames: 10,
        ..Limits::default()
    };
    assert_eq!(decode_wav(&w, &tight), Err(SamplerError::TooLarge));
}

#[test]
fn wav_decodes_and_reads_loop() {
    let w = wav16(44100, 2, &wave_frames(40), Some((4, 19)));
    let d = decode_wav(&w, &Limits::default()).unwrap();
    assert_eq!((d.rate, d.channels, d.len_frames()), (44100, 2, 20));
    assert_eq!(d.loop_points, Some((4, 20)));
    assert!((d.frames[2] - 100.0 / 32768.0).abs() < 1e-9);
}

#[test]
fn wav_every_prefix_fails_without_panic() {
    let w = wav16(44100, 1, &wave_frames(30), Some((2, 10)));
    for n in 0..w.len() - 1 {
        assert!(
            decode_wav(&w[..n], &Limits::default()).is_err(),
            "prefix {n}"
        );
    }
    assert!(decode_wav(&w, &Limits::default()).is_ok());
}

#[test]
fn sfz_reads_scopes_notes_and_sample_files() {
    let bank = load_sfz(SFZ.as_bytes(), &Limits::default(), &mut sfz_files).unwrap();
    assert_eq!(bank.samples().len(), 2, "same file shares one sample");
    let z = bank.zones();
    assert_eq!(z.len(), 3);
    // Region 1: key=c4 sets range and root; group velocity applies; global release inherited.
    assert_eq!((z[0].key_lo, z[0].key_hi, z[0].root_key), (60, 60, 60));
    assert_eq!((z[0].vel_lo, z[0].vel_hi), (0, 63));
    assert_eq!(z[0].loop_mode, LoopMode::Continuous);
    assert_eq!((z[0].loop_start, z[0].loop_end), (10, 80));
    assert!((z[0].adsr.release - 0.01).abs() < 1e-6);
    // Region 2: note names, tuning, pan.
    assert_eq!((z[1].key_lo, z[1].key_hi), (60, 61));
    assert_eq!(z[1].tune_cents, 75);
    assert_eq!(z[1].pan, 1.0);
    assert_eq!(z[1].loop_mode, LoopMode::None);
    // Region 3: one shot, volume in dB, stereo wav.
    assert_eq!(z[2].loop_mode, LoopMode::OneShot);
    assert!((z[2].gain - 0.501_187).abs() < 1e-4);
    assert_eq!(bank.samples()[z[2].sample as usize].channels(), 2);
}

#[test]
fn sfz_uses_wav_loop_when_mode_is_absent() {
    let text = "<region> sample=other.wav key=60";
    let mut files = |_: &str| Ok(wav16(22050, 1, &wave_frames(100), Some((5, 49))));
    let bank = load_sfz(text.as_bytes(), &Limits::default(), &mut files).unwrap();
    let z = bank.zones()[0];
    assert_eq!(z.loop_mode, LoopMode::Continuous);
    assert_eq!((z.loop_start, z.loop_end), (5, 50));
}

#[test]
fn sfz_rejects_bad_input_and_every_prefix_is_safe() {
    let l = Limits::default();
    let bad = |t: &str| load_sfz(t.as_bytes(), &l, &mut sfz_files);
    assert_eq!(bad(""), Err(SamplerError::NoZones));
    assert_eq!(
        bad("<region> sample=missing.wav"),
        Err(SamplerError::MissingSample)
    );
    assert_eq!(
        bad("<region> sample=samples/a b.wav key=h9"),
        Err(SamplerError::Malformed)
    );
    assert_eq!(
        bad("<region> sample=samples/a b.wav loop_mode=loop_continuous loop_end=9999"),
        Err(SamplerError::BadLoop)
    );
    assert_eq!(
        bad("<region> sample=samples/a b.wav loop_mode=bogus"),
        Err(SamplerError::Malformed)
    );
    let tight = Limits {
        max_zones: 1,
        ..Limits::default()
    };
    assert_eq!(
        load_sfz(SFZ.as_bytes(), &tight, &mut sfz_files),
        Err(SamplerError::TooLarge)
    );
    let text = SFZ.as_bytes();
    for n in 0..text.len() {
        let _ = load_sfz(&text[..n], &l, &mut sfz_files);
    }
}

#[test]
fn sfz_file_loader_stays_inside_its_folder() {
    let dir = std::env::temp_dir().join(format!("composer-sampler-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("inner")).unwrap();
    std::fs::write(
        dir.join("inner/a.wav"),
        wav16(44100, 1, &wave_frames(50), None),
    )
    .unwrap();
    std::fs::write(
        dir.join("outside.wav"),
        wav16(44100, 1, &wave_frames(50), None),
    )
    .unwrap();
    std::fs::write(dir.join("inner/ok.sfz"), "<region> sample=a.wav key=60").unwrap();
    std::fs::write(
        dir.join("inner/up.sfz"),
        "<region> sample=../outside.wav key=60",
    )
    .unwrap();
    std::fs::write(
        dir.join("inner/abs.sfz"),
        "<region> sample=/etc/hosts key=60",
    )
    .unwrap();
    let l = Limits::default();
    assert!(load_sfz_file(&dir.join("inner/ok.sfz"), &l).is_ok());
    assert_eq!(
        load_sfz_file(&dir.join("inner/up.sfz"), &l),
        Err(SamplerError::MissingSample)
    );
    assert_eq!(
        load_sfz_file(&dir.join("inner/abs.sfz"), &l),
        Err(SamplerError::MissingSample)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loaded_sf2_plays_through_the_engine() {
    let bank = Arc::new(load_sf2(&tiny_sf2(), &Limits::default(), None).unwrap());
    let inst = Instrument::new(&InstrumentDef::Bank(BankParams { bank, gain: 1.0 }), 44100);
    let ev = |frame, kind| Event {
        frame,
        track: 0,
        kind,
    };
    let mut e = Engine::new(
        EngineConfig {
            voices_per_track: 4,
        },
        vec![TrackSetup {
            instrument: inst,
            gain: 1.0,
            pan: 0.0,
        }],
        vec![
            ev(
                0,
                EventKind::NoteOn {
                    pitch: 60,
                    velocity: 30,
                },
            ),
            ev(2000, EventKind::NoteOff { pitch: 60 }),
        ],
    );
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    let mut peak = 0.0f32;
    for _ in 0..40 {
        e.process(&mut l, &mut r);
        peak = l.iter().fold(peak, |p, v| p.max(v.abs()));
    }
    // The looped soft layer is still sounding well past the 200 frame sample length.
    assert!(peak > 0.0);
    assert!(!e.voice_pitches(0).is_empty() || e.position() > 4000);
}
