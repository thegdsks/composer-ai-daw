//! Automation commands (D36): add, move, curve, delete, clear, track deletion and undo.
use super::*;
use composer_model::{AutomationCurve, AutomationTarget, Change, Id, Project, TrackKind};

fn id(n: u128) -> Id {
    Id(n)
}

fn editor() -> Editor {
    let mut ed = Editor::new(Project::new(id(1), "t"));
    ed.execute(&Command::AddTrack {
        id: id(10),
        name: "keys".into(),
        kind: TrackKind::Instrument,
        index: None,
    })
    .unwrap();
    ed
}

const VOL: AutomationTarget = AutomationTarget::TrackVolume { track: Id(10) };

fn add(tick: i64, value: i16) -> Command {
    Command::AddAutomationPoint {
        target: VOL,
        tick,
        value,
        curve: None,
    }
}

#[test]
fn no_lane_leaves_json_and_hash_alone() {
    let mut ed = editor();
    let before = ed.project().state_hash();
    assert!(!serde_json::to_string(ed.project())
        .unwrap()
        .contains("automation"));
    ed.execute(&add(0, 1000)).unwrap();
    assert_ne!(ed.project().state_hash(), before);
    assert!(serde_json::to_string(ed.project())
        .unwrap()
        .contains("automation"));
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), before);
}

#[test]
fn add_move_curve_delete_undo_exactly() {
    let mut ed = editor();
    ed.execute(&add(0, 1000)).unwrap();
    ed.execute(&add(960, 0)).unwrap();
    let h2 = ed.project().state_hash();
    // A second point on the same tick is refused.
    assert!(ed.execute(&add(960, 5)).is_err());
    ed.execute(&Command::MoveAutomationPoint {
        target: VOL,
        tick: 960,
        new_tick: 1920,
        value: 300,
    })
    .unwrap();
    let lane = ed.project().lane(&VOL).unwrap();
    assert_eq!(lane.points.keys().copied().collect::<Vec<_>>(), [0, 1920]);
    // Moving onto a taken tick fails and changes nothing.
    let h = ed.project().state_hash();
    assert!(ed
        .execute(&Command::MoveAutomationPoint {
            target: VOL,
            tick: 0,
            new_tick: 1920,
            value: 1,
        })
        .is_err());
    assert_eq!(ed.project().state_hash(), h);
    ed.execute(&Command::SetAutomationCurve {
        target: VOL,
        tick: 0,
        curve: AutomationCurve::Smooth,
    })
    .unwrap();
    assert_eq!(
        ed.project().automation_point(&VOL, 0).unwrap().curve,
        AutomationCurve::Smooth
    );
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h2, "a move is one undo step");
    ed.execute(&Command::DeleteAutomationPoint {
        target: VOL,
        tick: 0,
    })
    .unwrap();
    ed.execute(&Command::DeleteAutomationPoint {
        target: VOL,
        tick: 960,
    })
    .unwrap();
    assert!(
        ed.project().lane(&VOL).is_none(),
        "an empty lane is dropped"
    );
    ed.undo().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h2);
    while ed.undo().unwrap() {}
    assert!(ed.project().automation.is_empty());
    ed.project().validate().unwrap();
}

#[test]
fn bounds_per_target() {
    let mut ed = editor();
    assert!(ed.execute(&add(0, 2001)).is_err());
    assert!(ed.execute(&add(0, -1)).is_err());
    assert!(ed.execute(&add(-5, 10)).is_err());
    ed.execute(&add(0, 2000)).unwrap();
    let pan = AutomationTarget::TrackPan { track: id(10) };
    for (value, ok) in [
        (-1000i16, true),
        (1000, true),
        (1001, false),
        (-1001, false),
    ] {
        let r = ed.execute(&Command::AddAutomationPoint {
            target: pan,
            tick: i64::from(value) + 2000,
            value,
            curve: None,
        });
        assert_eq!(r.is_ok(), ok, "{value}");
    }
    let send = AutomationTarget::TrackReverbSend { track: id(10) };
    let at = |value| Command::AddAutomationPoint {
        target: send,
        tick: 0,
        value,
        curve: None,
    };
    assert!(ed.execute(&at(1001)).is_err());
    ed.execute(&at(1000)).unwrap();
    // A target on a track that does not exist is refused.
    assert!(ed
        .execute(&Command::AddAutomationPoint {
            target: AutomationTarget::TrackVolume { track: id(99) },
            tick: 0,
            value: 1,
            curve: None,
        })
        .is_err());
    ed.execute(&Command::AddAutomationPoint {
        target: AutomationTarget::MasterVolume,
        tick: 0,
        value: 700,
        curve: None,
    })
    .unwrap();
    ed.project().validate().unwrap();
}

#[test]
fn clear_and_delete_track_undo_in_one_step() {
    let mut ed = editor();
    for t in [0, 960, 1920] {
        ed.execute(&add(t, 500)).unwrap();
    }
    let h = ed.project().state_hash();
    let depth = ed.undo_depth();
    ed.execute(&Command::ClearAutomationLane { target: VOL })
        .unwrap();
    assert!(ed.project().automation.is_empty());
    assert_eq!(ed.undo_depth(), depth + 1);
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h);
    ed.execute(&Command::DeleteTrack { track: id(10) }).unwrap();
    assert!(ed.project().automation.is_empty());
    ed.project().validate().unwrap();
    ed.undo().unwrap();
    assert_eq!(ed.project().state_hash(), h);
    ed.redo().unwrap();
    assert!(ed.project().tracks.is_empty());
}

#[test]
fn the_model_refuses_to_drop_a_track_that_owns_lanes() {
    let mut ed = editor();
    ed.execute(&add(0, 500)).unwrap();
    let mut p = ed.project().clone();
    let track = p.tracks[0].clone();
    assert!(p.apply(&Change::DeleteTrack { index: 0, track }).is_err());
}

#[test]
fn json_round_trip_and_meta() {
    let mut ed = editor();
    ed.execute(&add(0, 100)).unwrap();
    let json = serde_json::to_string(ed.project()).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, ed.project());
    assert_eq!(back.state_hash(), ed.project().state_hash());
    let c: Command = serde_json::from_str(
        r#"{"command":"add_automation_point","target":{"kind":"master_volume"},"tick":0,"value":900}"#,
    )
    .unwrap();
    ed.execute(&c).unwrap();
    let entries: Vec<_> = commands_enabled(ed.project(), EnabledContext::default())
        .into_iter()
        .filter(|e| e.name.contains("automation"))
        .collect();
    assert_eq!(entries.len(), 5);
    assert!(entries.iter().all(|e| e.enabled));
    for name in [
        "add_automation_point",
        "move_automation_point",
        "set_automation_curve",
        "delete_automation_point",
        "clear_automation_lane",
    ] {
        assert!(command_meta(name).is_some(), "{name}");
    }
}
