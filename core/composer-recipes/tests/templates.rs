//! Template loading, placeholder resolution and replay.

mod common;

use composer_commands::{Command, Editor};
use composer_model::{Id, Project};
use composer_recipes::api::{self, TemplateRequest};
use composer_recipes::templates;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn every_file_in_templates_dir_is_bundled_and_parses() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates");
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .expect("templates dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".json").map(str::to_string))
        .collect();
    files.sort();
    let mut ids: Vec<&str> = templates::all().iter().map(|t| t.id).collect();
    ids.sort_unstable();
    assert_eq!(files, ids);
    assert_eq!(templates::all().len(), templates::source_count());
}

#[test]
fn templates_list_describes_each_template() {
    let v = api::templates_list();
    let list = v["templates"].as_array().expect("list");
    assert_eq!(list.len(), templates::all().len());
    let calm = list.iter().find(|t| t["id"] == "calm-hour").expect("calm");
    assert_eq!(calm["key"], "F major");
    assert_eq!(calm["tempo"], 60.0);
    assert!(calm["command_count"].as_u64().unwrap() > 20);
}

#[test]
fn key_text_parses_sharps_flats_and_rejects_nonsense() {
    let k = templates::parse_key("Bb minor").expect("key");
    assert_eq!((k.root, k.scale.name()), (10, "minor"));
    assert_eq!(templates::parse_key("F# dorian").expect("key").root, 6);
    assert!(templates::parse_key("H major").is_none());
    assert!(templates::parse_key("C").is_none());
    assert!(templates::parse_key("C lydian").is_none());
}

#[test]
fn a_template_resolves_to_real_ids_and_replays_to_the_same_hash() {
    for t in templates::all() {
        let empty = Project::new(Id(1), "Empty");
        let plan = templates::plan(&empty, t, 3).expect("plan");
        let text = serde_json::to_string(&plan.commands).expect("json");
        assert!(!text.contains('$'), "{}: a placeholder was left", t.id);
        assert!(plan.ids.contains_key("t1"));
        assert!(matches!(
            plan.commands.last(),
            Some(Command::SetKey { tick: 0, .. })
        ));
        let again = templates::plan(&empty, t, 3).expect("plan");
        assert_eq!(plan.commands, again.commands, "{}", t.id);
        let other = templates::plan(&empty, t, 4).expect("plan");
        assert_ne!(plan.commands, other.commands, "{}", t.id);

        let planned = api::plan_template(
            &empty,
            &TemplateRequest {
                template: t.id.into(),
                seed: Some(3),
                dry_run: false,
            },
        )
        .expect("planned");
        let mut grouped = Editor::new(empty.clone());
        api::apply(&mut grouped, &planned).expect("apply");
        assert_eq!(grouped.undo_depth(), 1, "{}", t.id);
        let mut stepped = Editor::new(empty.clone());
        for c in &plan.commands {
            stepped.execute(c).expect("command");
        }
        assert_eq!(
            grouped.project().state_hash(),
            stepped.project().state_hash(),
            "{}",
            t.id
        );
        grouped.undo().expect("undo");
        assert_eq!(grouped.project().state_hash(), empty.state_hash());
    }
}

#[test]
fn applying_a_template_twice_gives_fresh_ids() {
    let mut ed = Editor::new(Project::new(Id(1), "P"));
    for _ in 0..2 {
        let planned = api::plan_template(
            ed.project(),
            &TemplateRequest {
                template: "blank".into(),
                seed: None,
                dry_run: false,
            },
        )
        .expect("plan");
        api::apply(&mut ed, &planned).expect("apply");
    }
    assert_eq!(ed.project().tracks.len(), 2);
}

#[test]
fn unknown_template_is_a_clear_error() {
    let p = Project::new(Id(1), "P");
    let e = api::plan_template(
        &p,
        &TemplateRequest {
            template: "nope".into(),
            seed: None,
            dry_run: false,
        },
    )
    .unwrap_err();
    assert_eq!(e.code, "unknown_template");
    let r: Result<TemplateRequest, _> =
        serde_json::from_value(json!({ "template": "blank", "extra": 1 }));
    assert!(r.is_err());
}
