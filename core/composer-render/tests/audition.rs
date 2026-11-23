//! Writes one WAV per built in voice for a listening check (D29). Ignored by default so CI
//! never writes files: `COMPOSER_AUDITION_DIR=/some/dir cargo test -p composer-render
//! --test audition -- --ignored`.

use composer_engine::BuiltinVoice;
use composer_model::{
    Id, Note, Project, Region, RegionContent, Track, TrackKind, TICKS_PER_QUARTER,
};
use composer_render::{render_project, write_wav, RenderSettings, WavFormat};
use std::collections::BTreeMap;

const SR: u32 = 48_000;

/// Chords of the phrase: A minor, F, C, G, as MIDI pitches around the middle of the keyboard.
const CHORDS: [[u8; 3]; 4] = [[57, 60, 64], [53, 57, 60], [55, 60, 64], [55, 59, 62]];

fn phrase_track(id: u128, sound: &str, shift: i32, arpeggio: bool) -> Track {
    let bar = 4 * TICKS_PER_QUARTER;
    let mut notes = BTreeMap::new();
    let mut n = 0u128;
    for (b, chord) in CHORDS.iter().enumerate() {
        for (k, p) in chord.iter().enumerate() {
            let (start, dur) = if arpeggio {
                (b as i64 * bar + k as i64 * bar / 4, bar / 2)
            } else {
                (b as i64 * bar, bar - 120)
            };
            n += 1;
            let nid = Id(id * 1000 + n);
            notes.insert(
                nid,
                Note {
                    id: nid,
                    pitch: (i32::from(*p) + shift).clamp(0, 127) as u8,
                    velocity: 90 + 10 * k as u8,
                    start,
                    duration: dur,
                },
            );
        }
    }
    let region = Region {
        loop_length: 0,
        id: Id(id + 500),
        start: 0,
        length: 4 * bar,
        content: RegionContent::Midi { notes },
    };
    Track {
        id: Id(id),
        name: sound.to_string(),
        kind: TrackKind::Instrument,
        instrument: None,
        mix: Default::default(),
        sound: Some(sound.to_string()),
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    }
}

fn write(dir: &str, name: &str, p: &Project) {
    let r = render_project(p, &RenderSettings::new(SR, 3.0));
    let bytes = write_wav(WavFormat::Pcm16, 2, SR, &r.samples);
    std::fs::write(format!("{dir}/{name}.wav"), bytes).unwrap();
}

#[test]
#[ignore]
fn write_audition_wavs() {
    let dir = std::env::var("COMPOSER_AUDITION_DIR").expect("set COMPOSER_AUDITION_DIR");
    for v in BuiltinVoice::ALL {
        let (shift, arp) = match v {
            BuiltinVoice::SubBass => (-24, false),
            BuiltinVoice::Bell => (12, true),
            BuiltinVoice::Pluck | BuiltinVoice::SoftPulse => (0, true),
            _ => (0, false),
        };
        let mut p = Project::new(Id(1), v.sound_id());
        p.tracks.push(phrase_track(10, v.sound_id(), shift, arp));
        write(&dir, v.sound_id(), &p);
    }
    // All together, the way a first ambient sketch would sound.
    let mut p = Project::new(Id(1), "together");
    for (i, (v, shift, arp)) in [
        (BuiltinVoice::WarmPad, -12, false),
        (BuiltinVoice::WarmStrings, 0, false),
        (BuiltinVoice::SubBass, -24, false),
        (BuiltinVoice::SoftPiano, 12, true),
        (BuiltinVoice::Bell, 24, true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut t = phrase_track(10 + i as u128, v.sound_id(), shift, arp);
        t.mix.volume = 400;
        p.tracks.push(t);
    }
    write(&dir, "together", &p);
}
