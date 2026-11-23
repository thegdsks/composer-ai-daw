//! The demo project of the render tests, built through the model and compiled to a blob.

use crate::blob::compile_project;
use composer_engine::BUILTIN_SAW;
use composer_model::{Id, Note, Project, Region, RegionContent, Track, TrackKind};
use std::collections::BTreeMap;

fn track(id: u128, instrument: Option<Id>, notes: &[(u8, i64, i64)]) -> Track {
    let notes = notes
        .iter()
        .enumerate()
        .map(|(i, &(pitch, start, duration))| {
            let nid = Id(id * 10 + i as u128);
            (
                nid,
                Note {
                    id: nid,
                    pitch,
                    velocity: 100,
                    start,
                    duration,
                },
            )
        })
        .collect();
    let region = Region {
        loop_length: 0,
        id: Id(id + 1000),
        start: 0,
        length: 20_000,
        content: RegionContent::Midi { notes },
    };
    Track {
        id: Id(id),
        name: format!("t{id}"),
        kind: TrackKind::Instrument,
        instrument,
        mix: Default::default(),
        sound: None,
        reverb_send: 0,
        regions: BTreeMap::from([(region.id, region)]),
    }
}

/// Two tracks (saw and sine); 120 bpm, then 60 bpm from tick 3840.
pub fn demo_blob(sample_rate: u32, tail_seconds: f64) -> Vec<u8> {
    let mut p = Project::new(Id(1), "demo");
    p.tempo_map.tempos.insert(3840, 60_000);
    p.tracks.push(track(
        10,
        Some(BUILTIN_SAW),
        &[(60, 0, 960), (64, 3840, 960)],
    ));
    p.tracks
        .push(track(20, None, &[(48, 960, 1920), (55, 4800, 960)]));
    compile_project(&p, sample_rate, tail_seconds)
}
