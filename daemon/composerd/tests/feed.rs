//! Change feed (D25): feeds in replies and pushes, `changes.since`, fallbacks to a full fetch.

use composerd::{handle_line, ClientHandle, FileConfig, Hub};
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

    fn ok(&mut self, hub: &Hub, method: &str, params: Value) -> Value {
        self.n += 1;
        let line = json!({ "jsonrpc": "2.0", "id": self.n, "method": method, "params": params });
        handle_line(hub, &self.h, &line.to_string());
        loop {
            let v: Value = serde_json::from_str(&self.rx.recv().expect("reply")).unwrap();
            if v["id"] == json!(self.n) {
                assert!(v.get("error").is_none(), "{method}: {v}");
                return v["result"].clone();
            }
        }
    }

    fn pushes(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(s) = self.rx.try_recv() {
            out.push(serde_json::from_str(&s).unwrap());
        }
        out
    }
}

fn track(n: u32) -> Value {
    json!({ "command": "add_track", "id": format!("{n:032x}"), "name": format!("T{n}"), "kind": "instrument", "index": null })
}

fn head(c: &mut Client, hub: &Hub) -> Value {
    c.ok(hub, "state.hash", json!({}))["state_hash"].clone()
}

#[test]
fn feed_is_opt_in_and_chains_on_the_hash() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    // Without the flag the reply has no feed: the wire is unchanged.
    let plain = c.ok(&hub, "command.apply", json!({ "command": track(1) }));
    assert!(plain.get("changes").is_none());
    let h1 = plain["state_hash"].clone();
    let r = c.ok(
        &hub,
        "command.apply",
        json!({ "command": track(2), "changes": true }),
    );
    assert_eq!(r["changes"]["base_hash"], h1);
    assert_eq!(r["changes"]["hash"], r["state_hash"]);
    assert_eq!(r["changes"]["list"][0]["op"], "new_track");
    let g = c.ok(
        &hub,
        "command.group",
        json!({ "label": "g", "commands": [track(3), track(4)], "changes": true }),
    );
    assert_eq!(g["changes"]["list"].as_array().unwrap().len(), 2);
    let u = c.ok(&hub, "undo", json!({ "changes": true }));
    assert_eq!(u["changes"]["list"][0]["op"], "delete_track");
    assert_eq!(u["changes"]["base_hash"], g["state_hash"]);
    let rd = c.ok(&hub, "redo", json!({ "steps": 1, "changes": true }));
    assert_eq!(rd["changes"]["base_hash"], u["state_hash"]);
    let many = c.ok(&hub, "undo", json!({ "steps": 99, "changes": true }));
    assert!(many["changes"]["list"].as_array().unwrap().len() >= 2);
}

#[test]
fn state_changed_carries_the_feed_to_other_clients() {
    let hub = Hub::new(4);
    let mut a = Client::new(&hub);
    let mut b = Client::new(&hub);
    let before = head(&mut a, &hub);
    a.ok(&hub, "command.apply", json!({ "command": track(1) }));
    let pushes = b.pushes();
    let p = &pushes[0]["params"];
    assert_eq!(p["changes"]["base_hash"], before);
    assert_eq!(p["changes"]["hash"], p["hash"]);
    assert_eq!(p["changes"]["list"][0]["op"], "new_track");
}

#[test]
fn changes_since_joins_feeds_and_falls_back_to_full() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    let base = head(&mut c, &hub);
    for n in 1..=3 {
        c.ok(&hub, "command.apply", json!({ "command": track(n) }));
    }
    c.ok(&hub, "undo", json!({}));
    let r = c.ok(&hub, "changes.since", json!({ "hash": base }));
    assert_eq!(r["list"].as_array().unwrap().len(), 4);
    let now = head(&mut c, &hub);
    assert_eq!(r["hash"], now);
    let same = c.ok(&hub, "changes.since", json!({ "hash": now }));
    assert_eq!(same["list"], json!([]));
    let unknown = c.ok(&hub, "changes.since", json!({ "hash": "0123456789abcdef" }));
    assert_eq!(unknown["full"], true);
    // A session replacement in between forces a full fetch.
    c.ok(&hub, "session.new", json!({ "name": "fresh" }));
    let gone = c.ok(&hub, "changes.since", json!({ "hash": now }));
    assert_eq!(gone["full"], true);
}

#[test]
fn oversize_feeds_are_full() {
    let files = FileConfig {
        max_feed_changes: 2,
        ..FileConfig::default()
    };
    let hub = Hub::with_files(4, files);
    let mut c = Client::new(&hub);
    let big = c.ok(
        &hub,
        "command.group",
        json!({ "label": "big", "commands": [track(1), track(2), track(3)], "changes": true }),
    );
    assert_eq!(big["changes"]["full"], true);
    assert!(big["changes"].get("list").is_none());
    let small = c.ok(
        &hub,
        "command.apply",
        json!({ "command": track(4), "changes": true }),
    );
    assert!(small["changes"]["list"].is_array());
}

#[test]
fn rolled_back_group_pushes_the_inverse_changes() {
    let hub = Hub::new(4);
    let mut a = Client::new(&hub);
    let mut b = Client::new(&hub);
    let g = a.ok(&hub, "group.begin", json!({ "label": "trial" }));
    a.ok(&hub, "command.apply", json!({ "command": track(1) }));
    let live = head(&mut a, &hub);
    b.pushes();
    a.ok(&hub, "group.rollback", json!({ "token": g["token"] }));
    let pushes = b.pushes();
    let p = &pushes[0]["params"];
    assert_eq!(p["changes"]["base_hash"], live);
    assert_eq!(p["changes"]["list"][0]["op"], "delete_track");
}
