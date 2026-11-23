use crate::engine::{Event, EventKind};
use crate::tempo::TempoClock;
use composer_model::{Project, RegionContent};

/// Note events of a project plus where the last note ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub events: Vec<Event>,
    /// Frame of the latest note off, 0 when there are no notes.
    pub end_frame: u64,
    /// One entry per looped clip that repeats at least once (D24). The events are already
    /// expanded; this only tells clients where a loop starts and how long one pass lasts.
    pub loops: Vec<LoopSpan>,
}

/// A looped clip in frames, as compiled by [`compile_events`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopSpan {
    pub track: u32,
    /// Frame where the clip starts.
    pub start_frame: u64,
    /// Frames of the first pass.
    pub period_frames: u64,
    /// Frame where the clip ends.
    pub end_frame: u64,
}

/// Most passes one looped clip is expanded to, so a tiny loop in a long clip cannot exhaust memory.
pub const MAX_LOOP_PASSES: i64 = 100_000;

/// Turns the notes of every MIDI region into sample accurate events.
///
/// Track index in the events is the index in `project.tracks`. A note start is
/// `region.start + note.start`; its end is clamped to the region end, and notes
/// starting at or after the region end are dropped. A clip with a loop length plays only
/// the notes starting inside the first loop length, cut at the loop end, and repeats them
/// every loop length until the clip ends (D24); the expansion is done per pass in ticks so
/// tempo changes inside a clip are followed. Each tick is converted with
/// [`TempoClock`]. A note lasts at least one frame so on and off never collide.
pub fn compile_events(project: &Project, sample_rate: u32) -> Schedule {
    let clock = TempoClock::new(&project.tempo_map, sample_rate);
    let mut events = Vec::new();
    let mut end_frame = 0u64;
    let mut loops = Vec::new();
    for (ti, track) in project.tracks.iter().enumerate() {
        for region in track.regions.values() {
            let RegionContent::Midi { notes } = &region.content else {
                continue;
            };
            let region_end = region.start + region.length;
            let looped = region.loop_length > 0 && region.loop_length < region.length;
            let period = if looped {
                region.loop_length
            } else {
                region.length
            };
            if looped {
                loops.push(LoopSpan {
                    track: ti as u32,
                    start_frame: clock.frame_at(region.start),
                    period_frames: clock
                        .frame_at(region.start + period)
                        .saturating_sub(clock.frame_at(region.start)),
                    end_frame: clock.frame_at(region_end),
                });
            }
            let passes = if looped {
                ((region.length + period - 1) / period).min(MAX_LOOP_PASSES)
            } else {
                1
            };
            for pass in 0..passes {
                let origin = region.start + pass * period;
                let limit = if looped {
                    (origin + period).min(region_end)
                } else {
                    region_end
                };
                for note in notes.values() {
                    let start = origin + note.start;
                    let end = (start + note.duration).min(limit);
                    if note.start < 0 || note.start >= period || start >= limit || end <= start {
                        continue;
                    }
                    let on = clock.frame_at(start);
                    let off = clock.frame_at(end).max(on + 1);
                    end_frame = end_frame.max(off);
                    events.push(Event {
                        frame: on,
                        track: ti as u32,
                        kind: EventKind::NoteOn {
                            pitch: note.pitch,
                            velocity: note.velocity,
                        },
                    });
                    events.push(Event {
                        frame: off,
                        track: ti as u32,
                        kind: EventKind::NoteOff { pitch: note.pitch },
                    });
                }
            }
        }
    }
    Schedule {
        events,
        end_frame,
        loops,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use composer_model::{Id, Note, Region, Track, TrackKind};
    use std::collections::BTreeMap;

    fn project(length: i64, loop_length: i64) -> Project {
        let note = |id: u128, pitch: u8, start: i64, duration: i64| {
            (
                Id(id),
                Note {
                    id: Id(id),
                    pitch,
                    velocity: 90,
                    start,
                    duration,
                },
            )
        };
        let notes = BTreeMap::from([note(5, 60, 100, 300), note(6, 62, 600, 100)]);
        let region = Region {
            id: Id(3),
            start: 1000,
            length,
            loop_length,
            content: RegionContent::Midi { notes },
        };
        let mut p = Project::new(Id(1), "t");
        p.tracks.push(Track {
            id: Id(2),
            name: "t".into(),
            kind: TrackKind::Instrument,
            instrument: None,
            mix: Default::default(),
            sound: None,
            reverb_send: 0,
            regions: BTreeMap::from([(Id(3), region)]),
        });
        p
    }

    fn note_ons(s: &Schedule) -> Vec<(u64, u8)> {
        let mut v: Vec<(u64, u8)> = s
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::NoteOn { pitch, .. } => Some((e.frame, pitch)),
                _ => None,
            })
            .collect();
        v.sort();
        v
    }

    #[test]
    fn loop_repeats_first_pass_notes_and_cuts_at_the_loop_and_clip_end() {
        // Loop of 500 ticks: the note at 600 lies outside the loop and never plays.
        let p = project(1300, 500);
        let s = compile_events(&p, 48_000);
        let clock = TempoClock::new(&p.tempo_map, 48_000);
        let want: Vec<(u64, u8)> = [1100, 1600, 2100]
            .iter()
            .map(|t| (clock.frame_at(*t), 60))
            .collect();
        assert_eq!(note_ons(&s), want);
        assert_eq!(s.loops.len(), 1);
        assert_eq!(s.loops[0].start_frame, clock.frame_at(1000));
        assert_eq!(
            s.loops[0].period_frames,
            clock.frame_at(1500) - clock.frame_at(1000)
        );
        assert_eq!(s.loops[0].end_frame, clock.frame_at(2300));
    }

    #[test]
    fn no_loop_or_loop_covering_the_clip_is_the_plain_schedule() {
        let plain = compile_events(&project(1300, 0), 48_000);
        assert!(plain.loops.is_empty());
        assert_eq!(compile_events(&project(1300, 1300), 48_000), plain);
        assert_eq!(compile_events(&project(1300, 5000), 48_000), plain);
    }
}
