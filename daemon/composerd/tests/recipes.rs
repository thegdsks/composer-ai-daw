//! D28: template and recipe methods. The daemon and the wasm editor must answer alike for
//! the same input and seed, and every result must replay to the same hash.

use composer_wasm::EditorSession;
use composerd::{handle_line, ClientHandle, Hub};
use serde_json::{json, Value};
use std::sync::mpsc::Receiver;

struct Client {
    h: ClientHandle,
    rx: Receiver<String>,
    n: u64,
}

impl Client {
    fn new(hub: &Hub) -> Self {
        let (h, rx) = hub.connect().expect("connect");
        Self { h, rx, n: 0 }
    }

    fn call(&mut self, hub: &Hub, method: &str, params: Value) -> Value {
        self.n += 1;
        let line = json!({ "jsonrpc": "2.0", "id": self.n, "method": method, "params": params });
        handle_line(hub, &self.h, &line.to_string());
        loop {
            let v: Value = serde_json::from_str(&self.rx.recv().expect("reply")).unwrap();
            if v["id"] == json!(self.n) {
                return v;
            }
        }
    }

    fn ok(&mut self, hub: &Hub, method: &str, params: Value) -> Value {
        let r = self.call(hub, method, params);
        assert!(r.get("error").is_none(), "{method}: {r}");
        r["result"].clone()
    }
}

fn wasm_call(s: &mut EditorSession, method: &str, params: &Value) -> Value {
    let bytes = params.to_string();
    match method {
        "templates.list" => s.templates_list(),
        "recipes.list" => s.recipes_list(),
        "template.apply" => s.template_apply(bytes.as_bytes()),
        "recipe.plan" => s.recipe_plan(bytes.as_bytes()),
        "recipe.apply" => s.recipe_apply(bytes.as_bytes()),
        other => panic!("no wasm twin for {other}"),
    }
    serde_json::from_slice(s.result()).expect("json")
}

/// Both sides start from the same empty project.
fn pair() -> (Hub, Client, EditorSession) {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    c.ok(
        &hub,
        "session.new",
        json!({ "name": "t", "id": "ffffffffffffffffffffffffffffffff" }),
    );
    let s = EditorSession::new("t").expect("session");
    (hub, c, s)
}

fn same(hub: &Hub, c: &mut Client, s: &mut EditorSession, method: &str, params: Value) -> Value {
    let d = c.call(hub, method, params.clone());
    let w = wasm_call(s, method, &params);
    match (d.get("result"), w.get("result")) {
        (Some(dr), Some(wr)) => assert_eq!(dr, wr, "{method} {params}"),
        _ => {
            assert_eq!(
                d["error"]["data"], w["error"]["data"],
                "{method} {params}: {d} vs {w}"
            );
            assert_eq!(d["error"]["code"], w["error"]["code"], "{method} {params}");
        }
    }
    d
}

fn hash(hub: &Hub, c: &mut Client) -> Value {
    c.ok(hub, "state.hash", json!({}))["state_hash"].clone()
}

#[test]
fn lists_equal_the_wasm_editor() {
    let (hub, mut c, mut s) = pair();
    let t = same(&hub, &mut c, &mut s, "templates.list", json!({}));
    assert!(t["result"]["templates"].as_array().unwrap().len() >= 4);
    let r = same(&hub, &mut c, &mut s, "recipes.list", json!({}));
    let names: Vec<&str> = r["result"]["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "extend_to_length",
            "repeat_with_variation",
            "bass_from_chords",
            "pad_under_melody",
            "soften_velocity",
            "calm_sections"
        ]
    );
    let bad = c.call(&hub, "templates.list", json!({ "x": 1 }));
    assert_eq!(bad["error"]["data"]["code"], "invalid_params");
}

#[test]
fn template_then_recipes_match_between_daemon_and_wasm_and_undo_in_single_steps() {
    let (hub, mut c, mut s) = pair();
    let start = hash(&hub, &mut c);

    let applied = same(
        &hub,
        &mut c,
        &mut s,
        "template.apply",
        json!({ "template": "calm-hour", "seed": 5 }),
    );
    let r = &applied["result"];
    assert_eq!(r["template"], "calm-hour");
    assert_eq!(r["undo_depth"], 1);
    assert_eq!(r["undo_label"], "Template: Calm hour");
    let region = r["ids"]["r2"].as_str().expect("piano region").to_string();
    let after_template = hash(&hub, &mut c);
    assert_ne!(after_template, start);

    // Planning reads only: the hash does not move, and the list is what apply runs.
    let plan = same(
        &hub,
        &mut c,
        &mut s,
        "recipe.plan",
        json!({ "recipe": "repeat_with_variation", "scope": { "region": region },
                "params": { "copies": 2, "variation": 70 }, "seed": 9 }),
    );
    assert_eq!(hash(&hub, &mut c), after_template);
    let planned = plan["result"]["commands"].clone();
    assert_eq!(plan["result"]["applies"], true);
    assert!(planned.as_array().unwrap().len() > 2);

    let applied = same(
        &hub,
        &mut c,
        &mut s,
        "recipe.apply",
        json!({ "recipe": "repeat_with_variation", "scope": { "region": region },
                "params": { "copies": 2, "variation": 70 }, "seed": 9 }),
    );
    assert_eq!(applied["result"]["undo_depth"], 2);
    assert_eq!(
        applied["result"]["state_hash"], plan["result"]["state_hash_after"],
        "apply lands on the hash the plan promised"
    );
    let after_recipe = hash(&hub, &mut c);

    same(
        &hub,
        &mut c,
        &mut s,
        "recipe.apply",
        json!({ "recipe": "calm_sections", "seed": 3, "params": { "minutes": 2, "mood": "drifting" } }),
    );
    same(
        &hub,
        &mut c,
        &mut s,
        "recipe.apply",
        json!({ "recipe": "soften_velocity", "params": { "ceiling": 40, "amount": 80 } }),
    );
    let hist = c.ok(&hub, "history.get", json!({}));
    let sources: Vec<&str> = hist["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["template", "recipe", "recipe", "recipe"]);

    // Each apply is one undo step.
    c.ok(&hub, "undo", json!({}));
    c.ok(&hub, "undo", json!({}));
    assert_eq!(hash(&hub, &mut c), after_recipe);
    c.ok(&hub, "undo", json!({}));
    assert_eq!(hash(&hub, &mut c), after_template);
    c.ok(&hub, "undo", json!({}));
    assert_eq!(hash(&hub, &mut c), start);
}

#[test]
fn recipe_plan_commands_replay_through_command_group_to_the_same_hash() {
    let (hub, mut c, mut s) = pair();
    same(
        &hub,
        &mut c,
        &mut s,
        "template.apply",
        json!({ "template": "cinematic", "seed": 1 }),
    );
    let plan = c.ok(
        &hub,
        "recipe.plan",
        json!({ "recipe": "calm_sections", "seed": 77, "params": { "minutes": 1 } }),
    );
    let before = hash(&hub, &mut c);
    let grouped = c.ok(
        &hub,
        "command.group",
        json!({ "label": "replay", "commands": plan["commands"] }),
    );
    assert_eq!(grouped["state_hash"], plan["state_hash_after"]);
    assert_ne!(grouped["state_hash"], before);
    // And as a proposal: nothing changes until it is accepted.
    c.ok(&hub, "undo", json!({}));
    let posted = c.ok(
        &hub,
        "proposal.post",
        json!({ "label": "Calm", "commands": plan["commands"] }),
    );
    assert_eq!(posted["state_hash_after"], plan["state_hash_after"]);
    assert_eq!(hash(&hub, &mut c), before);
}

#[test]
fn dry_run_reports_and_changes_nothing() {
    let (hub, mut c, mut s) = pair();
    let before = hash(&hub, &mut c);
    let t = same(
        &hub,
        &mut c,
        &mut s,
        "template.apply",
        json!({ "template": "blank", "dry_run": true }),
    );
    assert_eq!(t["result"]["dry_run"], true);
    assert_eq!(hash(&hub, &mut c), before);
    let r = same(
        &hub,
        &mut c,
        &mut s,
        "recipe.apply",
        json!({ "recipe": "calm_sections", "dry_run": true, "seed": 2 }),
    );
    assert_eq!(r["result"]["dry_run"], true);
    assert_eq!(hash(&hub, &mut c), before);
}

#[test]
fn errors_use_the_same_codes_on_both_sides() {
    let (hub, mut c, mut s) = pair();
    let cases = [
        (
            "template.apply",
            json!({ "template": "nope" }),
            "unknown_template",
        ),
        (
            "template.apply",
            json!({ "template": "blank", "x": 1 }),
            "invalid_params",
        ),
        ("recipe.plan", json!({ "recipe": "nope" }), "unknown_recipe"),
        (
            "recipe.plan",
            json!({ "recipe": "extend_to_length" }),
            "no_target",
        ),
        (
            "recipe.plan",
            json!({ "recipe": "calm_sections", "params": { "minutes": 0 } }),
            "invalid_params",
        ),
        (
            "recipe.apply",
            json!({ "recipe": "soften_velocity" }),
            "nothing_to_do",
        ),
        (
            "recipe.plan",
            json!({ "recipe": "pad_under_melody", "scope": { "region": "00000000000000000000000000000009" } }),
            "no_target",
        ),
    ];
    for (method, params, code) in cases {
        let r = same(&hub, &mut c, &mut s, method, params.clone());
        assert_eq!(r["error"]["data"]["code"], code, "{method} {params}");
    }
}
