//! History read, step jumps, open groups and describe metadata (D20, D32, D33).

use composerd::{handle_line, ClientHandle, FileConfig, Hub};
use serde_json::{json, Value};
use std::sync::mpsc::Receiver;
use std::time::Duration;

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

    /// Sends one request and returns the reply, skipping pushes.
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

fn track(n: u32) -> Value {
    json!({ "command": "add_track", "id": format!("{n:032x}"), "name": format!("T{n}"), "kind": "instrument", "index": null })
}

fn hash(c: &mut Client, hub: &Hub) -> Value {
    c.ok(hub, "state.hash", json!({}))["state_hash"].clone()
}

#[test]
fn history_pages_sources_and_step_jumps() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    let mut hashes = vec![hash(&mut c, &hub)];
    for n in 1..=4 {
        c.ok(&hub, "command.apply", json!({ "command": track(n) }));
        hashes.push(hash(&mut c, &hub));
    }
    c.ok(
        &hub,
        "command.group",
        json!({ "label": "ask", "source": "ai", "commands": [track(5), track(6)] }),
    );
    let h = c.ok(&hub, "history.get", json!({}));
    assert_eq!(
        (h["cursor"].clone(), h["total"].clone()),
        (json!(5), json!(5))
    );
    assert_eq!(h["entries"][4]["source"], "ai");
    assert_eq!(h["entries"][4]["command_count"], 2);
    assert_eq!(h["entries"][0]["source"], "user");
    let page = c.ok(&hub, "history.get", json!({ "limit": 2, "offset": 3 }));
    assert_eq!(page["entries"].as_array().unwrap().len(), 2);
    assert_eq!(page["entries"][0]["index"], 3);

    let u = c.ok(&hub, "undo", json!({ "steps": 3 }));
    assert_eq!(u["steps_applied"], 3);
    assert_eq!(u["state_hash"], hashes[2]);
    let h = c.ok(&hub, "history.get", json!({}));
    assert_eq!(
        (h["cursor"].clone(), h["total"].clone()),
        (json!(2), json!(5))
    );
    let r = c.ok(&hub, "redo", json!({ "steps": 10 }));
    assert_eq!(r["steps_applied"], 3);

    let bad = c.call(&hub, "undo", json!({ "steps": 0 }));
    assert_eq!(bad["error"]["data"]["code"], "invalid_params");
    let bad = c.call(
        &hub,
        "command.group",
        json!({ "label": "x", "source": "robot", "commands": [track(9)] }),
    );
    assert_eq!(bad["error"]["data"]["code"], "invalid_params");
}

#[test]
fn history_limit_is_capped_from_config() {
    let files = FileConfig {
        max_history_page: 2,
        ..FileConfig::default()
    };
    let hub = Hub::with_files(4, files);
    let mut c = Client::new(&hub);
    for n in 1..=4 {
        c.ok(&hub, "command.apply", json!({ "command": track(n) }));
    }
    let h = c.ok(&hub, "history.get", json!({ "limit": 100 }));
    assert_eq!(h["entries"].as_array().unwrap().len(), 2);
    assert_eq!(h["total"], 4);
}

#[test]
fn open_group_commit_and_rollback() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    let h0 = hash(&mut c, &hub);
    let b = c.ok(
        &hub,
        "group.begin",
        json!({ "label": "trial", "source": "ai" }),
    );
    let token = b["token"].clone();
    assert!(token.is_string());
    c.ok(&hub, "command.apply", json!({ "command": track(1) }));
    assert_ne!(hash(&mut c, &hub), h0);
    // Undo is refused while a group is open; a second group too.
    assert_eq!(
        c.call(&hub, "undo", json!({}))["error"]["data"]["code"],
        "group_already_open"
    );
    assert_eq!(
        c.call(&hub, "group.begin", json!({ "label": "b" }))["error"]["data"]["code"],
        "group_already_open"
    );
    let wrong = c.call(
        &hub,
        "group.rollback",
        json!({ "token": format!("{:032x}", 7) }),
    );
    assert_eq!(wrong["error"]["data"]["code"], "group_token_mismatch");
    let r = c.ok(&hub, "group.rollback", json!({ "token": token }));
    assert_eq!(r["state_hash"], h0);
    assert_eq!(r["undo_depth"], 0);
    assert_eq!(
        c.call(&hub, "group.commit", json!({ "token": token }))["error"]["data"]["code"],
        "no_open_group"
    );

    let b = c.ok(&hub, "group.begin", json!({ "label": "keep" }));
    c.ok(&hub, "command.apply", json!({ "command": track(1) }));
    c.ok(&hub, "command.apply", json!({ "command": track(2) }));
    let r = c.ok(&hub, "group.commit", json!({ "token": b["token"] }));
    assert_eq!(
        (r["undo_depth"].clone(), r["undo_label"].clone()),
        (json!(1), json!("keep"))
    );
    let u = c.ok(&hub, "undo", json!({}));
    assert_eq!(u["state_hash"], h0);
}

#[test]
fn open_group_rolls_back_on_disconnect_and_timeout() {
    let hub = Hub::new(4);
    let mut a = Client::new(&hub);
    let mut watcher = Client::new(&hub);
    let h0 = hash(&mut a, &hub);
    a.ok(&hub, "group.begin", json!({ "label": "trial" }));
    a.ok(&hub, "command.apply", json!({ "command": track(1) }));
    hub.disconnect(&a.h);
    assert_eq!(hash(&mut watcher, &hub), h0);
    assert_eq!(watcher.ok(&hub, "history.get", json!({}))["total"], 0);

    let files = FileConfig {
        group_timeout_ms: 20,
        ..FileConfig::default()
    };
    let hub = Hub::with_files(4, files);
    let mut c = Client::new(&hub);
    let h0 = hash(&mut c, &hub);
    let b = c.ok(&hub, "group.begin", json!({ "label": "slow" }));
    c.ok(&hub, "command.apply", json!({ "command": track(1) }));
    std::thread::sleep(Duration::from_millis(40));
    // Any request notices the expiry and reverts first.
    assert_eq!(hash(&mut c, &hub), h0);
    let late = c.call(&hub, "group.commit", json!({ "token": b["token"] }));
    assert_eq!(late["error"]["data"]["code"], "no_open_group");
}

#[test]
fn describe_has_palette_metadata_and_enabled_gives_reasons() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    let d = c.ok(&hub, "commands.describe", json!({}));
    for e in d["commands"].as_array().unwrap() {
        assert!(e["label"].as_str().is_some_and(|s| !s.is_empty()), "{e}");
        assert!(e["verb"].is_string(), "{e}");
        assert!(!e["keywords"].as_array().unwrap().is_empty(), "{e}");
    }
    let en = c.ok(&hub, "commands.enabled", json!({}));
    let find = |v: &Value, n: &str| {
        v["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == n)
            .unwrap()
            .clone()
    };
    assert_eq!(find(&en, "add_track")["enabled"], true);
    let rn = find(&en, "rename_track");
    assert_eq!(rn["enabled"], false);
    assert!(rn["reason"].is_string());
    c.ok(&hub, "command.apply", json!({ "command": track(1) }));
    let en = c.ok(&hub, "commands.enabled", json!({}));
    assert_eq!(find(&en, "rename_track")["enabled"], true);
    let bad = c.call(
        &hub,
        "commands.enabled",
        json!({ "context": { "nope": 1 } }),
    );
    assert_eq!(bad["error"]["data"]["code"], "invalid_params");
}
