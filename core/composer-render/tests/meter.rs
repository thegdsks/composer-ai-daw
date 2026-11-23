use composer_model::{ChannelMix, Id, Note, Project, Region, RegionContent, Track, TrackKind};
use composer_render::{measure, mix_from_project, render_project, RenderSettings};
use std::collections::BTreeMap;

fn track(id: u128, mix: ChannelMix) -> Track {
    let note = Note {
        id: Id(id + 1),
        pitch: 60,
        velocity: 100,
        start: 0,
        duration: 960,
    };
    let region = Region {
        loop_length: 0,
        id: Id(id + 2),
        start: 0,
        length: 3840,
        content: RegionContent::Midi {
            notes: BTreeMap::from([(note.id, note)]),
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

fn project(a: ChannelMix, b: ChannelMix) -> Project {
    let mut p = Project::new(Id(1), "m");
    p.tracks.push(track(10, a));
    p.tracks.push(track(20, b));
    p.reindex();
    p
}

#[test]
fn project_mix_maps_volume_pan_mute_and_solo() {
    let loud = ChannelMix {
        volume: 500,
        pan: -1000,
        ..ChannelMix::default()
    };
    let m = mix_from_project(&project(loud, ChannelMix::default()));
    assert_eq!((m[&Id(10)].gain, m[&Id(10)].pan), (0.5, -1.0));
    assert_eq!(m[&Id(20)].gain, 1.0);
    let muted = ChannelMix {
        mute: true,
        ..ChannelMix::default()
    };
    assert_eq!(mix_from_project(&project(muted, loud))[&Id(10)].gain, 0.0);
    let solo = ChannelMix {
        solo: true,
        ..ChannelMix::default()
    };
    let m = mix_from_project(&project(solo, ChannelMix::default()));
    assert_eq!((m[&Id(10)].gain, m[&Id(20)].gain), (1.0, 0.0));
}

#[test]
fn render_follows_project_mix() {
    let p = project(ChannelMix::default(), ChannelMix::default());
    let base = RenderSettings::new(48_000, 0.1);
    let both = render_project(&p, &base.clone().with_project_mix(&p));
    let muted = ChannelMix {
        mute: true,
        ..ChannelMix::default()
    };
    let p2 = project(ChannelMix::default(), muted);
    let one = render_project(&p2, &base.with_project_mix(&p2));
    let peak = |r: &composer_render::Rendered| r.samples.iter().fold(0.0f32, |a, s| a.max(s.abs()));
    assert!(peak(&both) > peak(&one) * 1.5);
}

#[test]
fn measure_reports_master_and_per_track_levels() {
    let muted = ChannelMix {
        mute: true,
        ..ChannelMix::default()
    };
    let p = project(ChannelMix::default(), muted);
    let r = measure(&p, &RenderSettings::new(48_000, 0.1));
    assert_eq!(r.tracks.len(), 2);
    assert!(r.master.peak > 0.0 && r.master.rms > 0.0);
    assert!(r.tracks[0].1.peak > 0.0);
    assert_eq!(r.tracks[1].1.peak, 0.0, "muted track is silent");
    // With one audible track the master equals that track.
    assert!((r.master.peak - r.tracks[0].1.peak).abs() < 1e-6);
    assert_eq!(r, measure(&p, &RenderSettings::new(48_000, 0.1)));
}

#[test]
fn empty_project_measures_silence() {
    let p = Project::new(Id(1), "e");
    let r = measure(&p, &RenderSettings::new(48_000, 0.0));
    assert_eq!((r.frames, r.master.peak, r.tracks.len()), (0, 0.0, 0));
}

#[test]
fn project_master_and_send_reach_the_render() {
    use composer_model::{Change, FieldKey, FieldValue, MasterMix};
    let mut p = project(ChannelMix::default(), ChannelMix::default());
    let settings = |p: &Project| RenderSettings::new(48_000, 0.5).with_project_mix(p);
    let dry = render_project(&p, &settings(&p)).samples;
    p.apply(&Change::Set {
        key: FieldKey::Master,
        old: FieldValue::Master(p.master),
        new: FieldValue::Master(MasterMix {
            volume: 1000,
            reverb_size: 700,
            reverb_mix: 500,
        }),
    })
    .unwrap();
    // The return is on but nothing is sent yet, so the output is unchanged.
    assert_eq!(render_project(&p, &settings(&p)).samples, dry);
    p.apply(&Change::Set {
        key: FieldKey::TrackReverbSend { track: Id(10) },
        old: FieldValue::Permille(0),
        new: FieldValue::Permille(800),
    })
    .unwrap();
    assert_ne!(render_project(&p, &settings(&p)).samples, dry);
}
