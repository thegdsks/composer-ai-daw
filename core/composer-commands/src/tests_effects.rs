//! Effect chain commands (D45): add, remove, move, bypass, param, track deletion and undo.
use super::*;
use composer_model::{EffectKind, EffectOwner, Id, Project, TrackKind};

fn editor() -> Editor {
    let mut ed = Editor::new(Project::new(Id(1), "t"));
    for (n, kind) in [(10, TrackKind::Instrument), (11, TrackKind::Audio)] {
        ed.execute(&Command::AddTrack {
            id: Id(n),
            name: "t".into(),
            kind,
            index: None,
        })
        .unwrap();
    }
    ed
}

const T: EffectOwner = EffectOwner::Track { track: Id(10) };

fn add(id: u128, owner: EffectOwner, kind: EffectKind) -> Command {
    Command::AddEffect {
        id: Id(id),
        owner,
        kind,
        index: None,
    }
}

#[test]
fn default_project_json_ignores_effects() {
    let ed = editor();
    assert!(!serde_json::to_string(ed.project())
        .unwrap()
        .contains("effects"));
}

#[test]
fn add_set_move_bypass_remove_and_undo_everything() {
    let mut ed = editor();
    let h0 = ed.project().state_hash();
    ed.execute(&add(20, T, EffectKind::Gate)).unwrap();
    ed.execute(&add(21, T, EffectKind::Delay)).unwrap();
    ed.execute(&add(22, EffectOwner::Master, EffectKind::Crusher))
        .unwrap();
    ed.execute(&Command::SetEffectParam {
        effect: Id(21),
        param: "time_ms".into(),
        value: 400,
    })
    .unwrap();
    ed.execute(&Command::SetEffectBypass {
        effect: Id(20),
        bypass: true,
    })
    .unwrap();
    ed.execute(&Command::MoveEffect {
        effect: Id(21),
        index: 0,
    })
    .unwrap();
    let chain = ed.project().chain(&T);
    assert_eq!(chain[0].id, Id(21));
    assert_eq!(chain[0].params[0], 400);
    assert!(chain[1].bypass);
    ed.project().validate().unwrap();
    assert!(ed.project().index_is_consistent());
    let json = serde_json::to_string(ed.project()).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, ed.project());
    assert_eq!(back.state_hash(), ed.project().state_hash());
    ed.execute(&Command::RemoveEffect { effect: Id(22) })
        .unwrap();
    ed.undo_steps(7).unwrap();
    assert_eq!(ed.project().state_hash(), h0);
    assert!(ed.project().effects.is_empty());
    ed.redo_steps(7).unwrap();
    ed.project().validate().unwrap();
}

#[test]
fn rejects_bad_input_and_leaves_the_project_alone() {
    let mut ed = editor();
    ed.execute(&add(20, T, EffectKind::Gate)).unwrap();
    let h = ed.project().state_hash();
    let bad = [
        add(20, T, EffectKind::Delay),
        add(30, EffectOwner::Track { track: Id(11) }, EffectKind::Delay),
        add(30, EffectOwner::Track { track: Id(99) }, EffectKind::Delay),
        Command::AddEffect {
            id: Id(30),
            owner: T,
            kind: EffectKind::Delay,
            index: Some(5),
        },
        Command::RemoveEffect { effect: Id(99) },
        Command::MoveEffect {
            effect: Id(20),
            index: 3,
        },
        Command::SetEffectParam {
            effect: Id(20),
            param: "nope".into(),
            value: 1,
        },
        Command::SetEffectParam {
            effect: Id(20),
            param: "release_ms".into(),
            value: 5000,
        },
        Command::SetEffectParam {
            effect: Id(20),
            param: "release_ms".into(),
            value: i32::MAX,
        },
    ];
    for c in bad {
        assert!(ed.execute(&c).is_err(), "{c:?}");
    }
    assert_eq!(ed.project().state_hash(), h);
}

#[test]
fn chain_is_capped() {
    let mut ed = editor();
    for i in 0..16 {
        ed.execute(&add(100 + i, T, EffectKind::Tidal)).unwrap();
    }
    assert!(ed.execute(&add(200, T, EffectKind::Tidal)).is_err());
}

#[test]
fn deleting_a_track_takes_its_chain_and_undo_brings_it_back() {
    let mut ed = editor();
    ed.execute(&add(20, T, EffectKind::Gate)).unwrap();
    ed.execute(&add(21, T, EffectKind::Delay)).unwrap();
    let before = ed.project().clone();
    ed.execute(&Command::DeleteTrack { track: Id(10) }).unwrap();
    assert!(ed.project().effects.is_empty());
    assert!(!ed.project().contains_id(Id(20)));
    ed.undo().unwrap();
    assert_eq!(ed.project(), &before);
    assert_eq!(ed.project().state_hash(), before.state_hash());
    ed.project().validate().unwrap();
}

#[test]
fn effect_ids_share_the_id_space() {
    let mut ed = editor();
    ed.execute(&add(20, T, EffectKind::Gate)).unwrap();
    let r = ed.execute(&Command::AddTrack {
        id: Id(20),
        name: "x".into(),
        kind: TrackKind::Instrument,
        index: None,
    });
    assert!(r.is_err());
}
