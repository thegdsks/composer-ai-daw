use super::*;
use composer_model::{Id, Project, TrackKind};

fn editor() -> Editor {
    Editor::new(Project::new(Id(1), "t"))
}

fn add_track(n: u128) -> Command {
    Command::AddTrack {
        id: Id(n),
        name: format!("t{n}"),
        kind: TrackKind::Instrument,
        index: None,
    }
}

#[test]
fn history_pages_and_cursor() {
    let mut ed = editor();
    for n in 10..15 {
        ed.execute(&add_track(n)).unwrap();
    }
    ed.undo_steps(2).unwrap();
    let all = ed.history(0, 100);
    assert_eq!((all.cursor, all.total, all.entries.len()), (3, 5, 5));
    assert!(all.entries.iter().enumerate().all(|(i, e)| e.index == i));
    let page = ed.history(3, 1);
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].index, 3);
    assert_eq!(ed.history(9, 5).entries.len(), 0);
}

#[test]
fn undo_steps_three_then_redo_returns_to_same_hash() {
    let mut ed = editor();
    let mut hashes = vec![ed.project().state_hash()];
    for n in 10..15 {
        ed.execute(&add_track(n)).unwrap();
        hashes.push(ed.project().state_hash());
    }
    assert_eq!(ed.undo_steps(3).unwrap(), 3);
    assert_eq!(ed.project().state_hash(), hashes[2]);
    assert_eq!(ed.undo_steps(10).unwrap(), 2);
    assert_eq!(ed.project().state_hash(), hashes[0]);
    assert_eq!(ed.redo_steps(4).unwrap(), 4);
    assert_eq!(ed.project().state_hash(), hashes[4]);
}

#[test]
fn group_source_and_command_count() {
    let mut ed = editor();
    ed.begin_group_as("ask", Source::Ai).unwrap();
    ed.execute(&add_track(10)).unwrap();
    ed.execute(&add_track(11)).unwrap();
    ed.commit_group().unwrap();
    ed.execute(&add_track(12)).unwrap();
    let h = ed.history(0, 10);
    assert_eq!(h.entries[0].source, Source::Ai);
    assert_eq!(h.entries[0].command_count, 2);
    assert_eq!(h.entries[1].source, Source::User);
    assert_eq!(h.entries[1].command_count, 1);
    assert_eq!(Source::parse("template"), Some(Source::Template));
    assert_eq!(Source::parse("x"), None);
}

#[test]
fn rollback_restores_hash_and_adds_no_step() {
    let mut ed = editor();
    let h0 = ed.project().state_hash();
    ed.begin_group("trial").unwrap();
    ed.execute(&add_track(10)).unwrap();
    assert!(ed.group_open());
    assert_ne!(ed.project().state_hash(), h0);
    ed.rollback_group().unwrap();
    assert_eq!(ed.project().state_hash(), h0);
    assert_eq!(ed.undo_depth(), 0);
}

#[test]
fn every_command_has_meta() {
    for c in describe() {
        let m = command_meta(c.name).unwrap_or_else(|| panic!("no meta for {}", c.name));
        assert!(["add", "change", "remove", "go_to", "suggest"].contains(&m.verb));
        assert!(!m.label.is_empty() && !m.keywords.is_empty());
        assert!(!m.label.contains('_'));
    }
}

#[test]
fn enabled_reasons_follow_project() {
    let mut ed = editor();
    let list = commands_enabled(ed.project(), EnabledContext::default());
    assert_eq!(list.len(), describe().len());
    let get = |l: &[EnabledEntry], n: &str| *l.iter().find(|e| e.name == n).unwrap();
    assert!(get(&list, "add_track").enabled);
    let rt = get(&list, "rename_track");
    assert!(!rt.enabled && rt.reason.is_some());
    ed.execute(&add_track(10)).unwrap();
    let list = commands_enabled(ed.project(), EnabledContext::default());
    assert!(get(&list, "rename_track").enabled);
    assert!(!get(&list, "delete_region").enabled);
    let ctx = EnabledContext {
        track: Some(Id(99)),
        region: None,
    };
    assert!(!get(&commands_enabled(ed.project(), ctx), "rename_track").enabled);
}

#[test]
fn journal_replays_to_the_same_project() {
    use composer_model::{Id, Project, TrackKind};
    let mut ed = Editor::new(Project::new(Id(1), "p"));
    ed.track_changes(true);
    let mut mirror = ed.project().clone();
    let add = |n: u128| Command::AddTrack {
        id: Id(n),
        name: format!("t{n}"),
        kind: TrackKind::Instrument,
        index: None,
    };
    ed.execute(&add(2)).unwrap();
    ed.execute(&add(3)).unwrap();
    ed.undo().unwrap();
    ed.begin_group("g").unwrap();
    ed.execute(&add(4)).unwrap();
    ed.rollback_group().unwrap();
    ed.undo().unwrap();
    ed.redo().unwrap();
    for c in ed.take_journal() {
        mirror.apply(&c).unwrap();
    }
    assert_eq!(mirror.state_hash(), ed.project().state_hash());
    assert!(ed.take_journal().is_empty());
}

#[test]
fn journal_is_empty_unless_tracking() {
    use composer_model::{Id, Project};
    let mut ed = Editor::new(Project::new(Id(1), "p"));
    ed.execute(&Command::SetProjectName { name: "x".into() })
        .unwrap();
    assert!(ed.take_journal().is_empty());
}
