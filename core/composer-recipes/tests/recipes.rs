//! Golden command lists per recipe and seed, determinism, parameter ranges and the
//! dry run of every recipe on every template.

mod common;

use common::{fixture, golden, CHORDS_REGION, CHORDS_TRACK, MELODY_REGION};
use composer_commands::{Command, Editor};
use composer_model::{Id, Project};
use composer_recipes::api::{self, RecipeRequest};
use composer_recipes::{recipes, templates, RecipeError, Scope};
use serde_json::{json, Value};

fn region(id: Id) -> Scope {
    Scope {
        track: None,
        region: Some(id),
    }
}

fn plan(
    p: &Project,
    recipe: &str,
    params: Value,
    scope: Scope,
    seed: u64,
) -> Result<Vec<Command>, RecipeError> {
    recipes::plan(p, recipe, params, scope, seed).map(|r| r.commands)
}

struct Case {
    name: &'static str,
    recipe: &'static str,
    params: Value,
    scope: Scope,
    seed: u64,
}

fn cases() -> Vec<Case> {
    let chords = region(CHORDS_REGION);
    let melody = region(MELODY_REGION);
    let mut v = vec![Case {
        name: "extend_16_bars",
        recipe: "extend_to_length",
        params: json!({ "bars": 16 }),
        scope: chords,
        seed: 0,
    }];
    for seed in [1, 7] {
        v.push(Case {
            name: if seed == 1 {
                "repeat_seed1"
            } else {
                "repeat_seed7"
            },
            recipe: "repeat_with_variation",
            params: json!({ "copies": 2, "variation": 60 }),
            scope: melody,
            seed,
        });
        v.push(Case {
            name: if seed == 1 {
                "bass_fifth_seed1"
            } else {
                "bass_fifth_seed7"
            },
            recipe: "bass_from_chords",
            params: json!({ "style": "root_fifth", "window_beats": 4 }),
            scope: chords,
            seed,
        });
        v.push(Case {
            name: if seed == 1 { "pad_seed1" } else { "pad_seed7" },
            recipe: "pad_under_melody",
            params: json!({ "bars_per_chord": 1, "color": 50 }),
            scope: melody,
            seed,
        });
        v.push(Case {
            name: if seed == 1 {
                "calm_seed1"
            } else {
                "calm_seed7"
            },
            recipe: "calm_sections",
            params: json!({ "minutes": 2, "root": 5, "scale": "major", "mood": "gentle" }),
            scope: Scope::default(),
            seed,
        });
    }
    v.push(Case {
        name: "bass_sustain_seed0",
        recipe: "bass_from_chords",
        params: json!({}),
        scope: chords,
        seed: 0,
    });
    v.push(Case {
        name: "soften_project",
        recipe: "soften_velocity",
        params: json!({ "amount": 50, "ceiling": 64 }),
        scope: Scope::default(),
        seed: 0,
    });
    v.push(Case {
        name: "calm_minor_drifting",
        recipe: "calm_sections",
        params: json!({ "minutes": 2, "root": 9, "scale": "minor", "mood": "drifting", "tempo": 50 }),
        scope: Scope::default(),
        seed: 3,
    });
    v
}

#[test]
fn golden_command_lists_per_recipe_and_seed() {
    let p = fixture();
    for c in cases() {
        let cmds = plan(&p, c.recipe, c.params.clone(), c.scope, c.seed)
            .unwrap_or_else(|e| panic!("{}: {e}", c.name));
        assert!(!cmds.is_empty(), "{}", c.name);
        golden(c.name, &serde_json::to_value(&cmds).expect("json"));
    }
}

#[test]
fn every_golden_list_applies_and_undoes_as_one_group() {
    let p = fixture();
    for c in cases() {
        let req = RecipeRequest {
            recipe: c.recipe.into(),
            params: c.params.clone(),
            scope: Some(c.scope),
            seed: Some(c.seed),
            dry_run: false,
        };
        let planned = api::plan_recipe(&p, &req).expect("plan");
        let mut ed = Editor::new(p.clone());
        let before = ed.project().state_hash();
        api::apply(&mut ed, &planned).unwrap_or_else(|e| panic!("{}: {:?}", c.name, e.error));
        assert_eq!(ed.undo_depth(), 1, "{}", c.name);
        let after = ed.project().state_hash();
        assert_ne!(before, after, "{}", c.name);
        // The same list run one command at a time reaches the same document.
        let mut replay = Editor::new(p.clone());
        for cmd in &planned.commands {
            replay.execute(cmd).expect("replay");
        }
        assert_eq!(replay.project().state_hash(), after, "{}", c.name);
        // The dry run report agrees with the real result.
        let rep = api::report(&p, &planned).expect("report");
        assert_eq!(rep["state_hash_after"], json!(format!("{after:016x}")));
        ed.undo().expect("undo");
        assert_eq!(ed.project().state_hash(), before, "{}", c.name);
    }
}

#[test]
fn same_inputs_same_commands_and_other_seeds_differ() {
    let p = fixture();
    for c in cases() {
        let a = plan(&p, c.recipe, c.params.clone(), c.scope, c.seed).unwrap();
        let b = plan(&p, c.recipe, c.params.clone(), c.scope, c.seed).unwrap();
        assert_eq!(a, b, "{}", c.name);
    }
    for (recipe, params, scope) in [
        ("calm_sections", json!({ "minutes": 2 }), Scope::default()),
        (
            "repeat_with_variation",
            json!({ "variation": 80 }),
            region(MELODY_REGION),
        ),
        (
            "pad_under_melody",
            json!({ "color": 60 }),
            region(MELODY_REGION),
        ),
    ] {
        let a = plan(&p, recipe, params.clone(), scope, 11).unwrap();
        let b = plan(&p, recipe, params, scope, 12).unwrap();
        assert_ne!(a, b, "{recipe}: seeds 11 and 12 gave the same list");
    }
}

#[test]
fn ids_never_collide_with_the_project_or_each_other() {
    let p = fixture();
    let mut ed = Editor::new(p.clone());
    // The same recipe and seed twice in a row: the second list gets fresh ids.
    for _ in 0..2 {
        let req = RecipeRequest {
            recipe: "calm_sections".into(),
            params: json!({ "minutes": 1 }),
            scope: None,
            seed: Some(5),
            dry_run: false,
        };
        let planned = api::plan_recipe(ed.project(), &req).expect("plan");
        api::apply(&mut ed, &planned).expect("apply");
    }
    assert_eq!(ed.undo_depth(), 2);
}

#[test]
fn bad_requests_get_clear_codes() {
    let p = fixture();
    let err = |recipe: &str, params: Value, scope: Scope| {
        recipes::plan(&p, recipe, params, scope, 0)
            .unwrap_err()
            .code
    };
    assert_eq!(err("nope", json!({}), Scope::default()), "unknown_recipe");
    assert_eq!(
        err(
            "extend_to_length",
            json!({ "bars": 0 }),
            region(CHORDS_REGION)
        ),
        "invalid_params"
    );
    assert_eq!(
        err("extend_to_length", json!({ "x": 1 }), region(CHORDS_REGION)),
        "invalid_params"
    );
    assert_eq!(
        err("extend_to_length", json!({}), Scope::default()),
        "no_target"
    );
    assert_eq!(
        err(
            "extend_to_length",
            json!({ "bars": 2 }),
            region(CHORDS_REGION)
        ),
        "nothing_to_do"
    );
    assert_eq!(
        err("extend_to_length", json!([]), region(CHORDS_REGION)),
        "invalid_params"
    );
    assert_eq!(
        err("calm_sections", json!({ "root": 3 }), Scope::default()),
        "invalid_params"
    );
    assert_eq!(
        err(
            "calm_sections",
            json!({ "section_bars": 10 }),
            Scope::default()
        ),
        "invalid_params"
    );
    assert_eq!(
        err(
            "bass_from_chords",
            json!({ "style": "x" }),
            region(CHORDS_REGION)
        ),
        "invalid_params"
    );
    assert_eq!(
        err(
            "soften_velocity",
            json!({ "ceiling": 127 }),
            Scope::default()
        ),
        "nothing_to_do"
    );
    // A project with no key cannot place a pad until a key is given.
    let mut bare = Editor::new(p.clone());
    bare.execute(&Command::RemoveKey { tick: 0 })
        .expect("remove key");
    let bare = bare.into_project();
    let e = recipes::plan(
        &bare,
        "pad_under_melody",
        json!({}),
        region(MELODY_REGION),
        0,
    )
    .unwrap_err();
    assert_eq!(e.code, "no_key");
    assert!(recipes::plan(
        &bare,
        "pad_under_melody",
        json!({ "root": 0, "scale": "major" }),
        region(MELODY_REGION),
        0
    )
    .is_ok());
    // A chord track is a fine scope too: its latest clip is used.
    let by_track = Scope {
        track: Some(CHORDS_TRACK),
        region: None,
    };
    assert!(recipes::plan(&p, "bass_from_chords", json!({}), by_track, 0).is_ok());
}

#[test]
fn every_listed_parameter_is_accepted_at_its_bounds() {
    let p = fixture();
    let list = recipes::list();
    for r in list.as_array().expect("list") {
        let name = r["name"].as_str().expect("name");
        for param in r["params"].as_array().expect("params") {
            let pn = param["name"].as_str().expect("param");
            let mut values: Vec<Value> = Vec::new();
            match param["type"].as_str().expect("type") {
                "integer" => {
                    values.push(param["min"].clone());
                    values.push(param["max"].clone());
                }
                "choice" => values.extend(param["choices"].as_array().expect("choices").clone()),
                _ => {}
            }
            for v in values {
                let mut params = json!({});
                params[pn] = v.clone();
                if pn == "root" {
                    params["scale"] = json!("major");
                }
                if pn == "scale" {
                    params["root"] = json!(0);
                }
                if pn == "minutes" && v == json!(120) {
                    params["tempo"] = json!(100);
                }
                match plan(&p, name, params, region(MELODY_REGION), 0) {
                    Ok(_) => {}
                    Err(e) => assert!(!e.is_invalid_params(), "{name} {pn}={v}: {e}"),
                }
            }
        }
    }
}

#[test]
fn calm_sections_covers_every_scale_and_mood_and_the_longest_piece() {
    let p = fixture();
    for scale in composer_model::Scale::ALL {
        for mood in ["still", "gentle", "drifting"] {
            let cmds = plan(
                &p,
                "calm_sections",
                json!({ "minutes": 3, "root": 2, "scale": scale.name(), "mood": mood }),
                Scope::default(),
                9,
            )
            .expect("plan");
            api::check(&p, &cmds)
                .unwrap_or_else(|e| panic!("{} {mood}: {:?}", scale.name(), e.error));
        }
    }
    let long = plan(
        &p,
        "calm_sections",
        json!({ "minutes": 120, "tempo": 100, "section_bars": 8 }),
        Scope::default(),
        1,
    )
    .expect("plan");
    api::check(&p, &long).expect("120 minutes applies");
    // 120 minutes at 100 bpm is 12000 beats, 375 sections of 8 bars.
    let markers = long
        .iter()
        .filter(|c| matches!(c, Command::SetMarker { .. }))
        .count();
    assert_eq!(markers, 375);
}

fn template_project(id: &str) -> Project {
    let t = templates::get(id).expect("template");
    let p = Project::new(Id(1), "T");
    let plan = templates::plan(&p, t, 0).expect("plan");
    let mut ed = Editor::new(p);
    for c in &plan.commands {
        ed.execute(c).expect("template command");
    }
    ed.into_project()
}

#[test]
fn every_recipe_plan_passes_the_dry_run_on_each_template() {
    let params = |recipe: &str| match recipe {
        "soften_velocity" => json!({ "ceiling": 30 }),
        "repeat_with_variation" => json!({ "variation": 70 }),
        "extend_to_length" => json!({ "bars": 64 }),
        _ => json!({}),
    };
    let mut ok_per_recipe = std::collections::BTreeMap::new();
    for t in templates::all() {
        let p = template_project(t.id);
        for recipe in recipes::names() {
            let mut scopes: Vec<Scope> = p
                .tracks
                .iter()
                .flat_map(|t| t.regions.keys())
                .map(|id| region(*id))
                .collect();
            scopes.push(Scope::default());
            for scope in scopes {
                for seed in [0u64, 4] {
                    match plan(&p, recipe, params(recipe), scope, seed) {
                        Ok(cmds) => {
                            api::check(&p, &cmds).unwrap_or_else(|e| {
                                panic!("{recipe} on {} seed {seed}: {:?}", t.id, e.error)
                            });
                            *ok_per_recipe.entry(recipe).or_insert(0) += 1;
                        }
                        Err(e) => assert!(
                            matches!(e.code, "no_target" | "nothing_to_do"),
                            "{recipe} on {}: {e}",
                            t.id
                        ),
                    }
                }
            }
        }
    }
    for recipe in recipes::names() {
        assert!(
            ok_per_recipe.get(recipe).copied().unwrap_or(0) > 0,
            "{recipe} never planned on any template"
        );
    }
}
