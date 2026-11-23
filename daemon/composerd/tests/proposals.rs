//! The proposals channel (D27): post, list and resolve, with state asserted after each step.

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

    fn code(&mut self, hub: &Hub, method: &str, params: Value) -> Value {
        let r = self.call(hub, method, params);
        r["error"]["data"]["code"].clone()
    }
}

fn track(n: u32) -> Value {
    json!({ "command": "add_track", "id": format!("{n:032x}"), "name": format!("T{n}"), "kind": "instrument", "index": null })
}

#[test]
fn post_changes_nothing_and_accept_is_one_ai_step() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    let h0 = c.ok(&hub, "state.hash", json!({}))["state_hash"].clone();
    let posted = c.ok(
        &hub,
        "proposal.post",
        json!({ "label": "Two tracks", "note": "n", "author": "bot", "commands": [track(1), track(2)] }),
    );
    assert_eq!(posted["id"], "p1");
    assert_eq!(posted["status"], "pending");
    assert_eq!(posted["state_hash"], h0);
    assert_eq!(c.ok(&hub, "state.hash", json!({}))["state_hash"], h0);
    assert_eq!(
        c.ok(&hub, "history.get", json!({}))["total"],
        0,
        "post adds no history"
    );

    let list = c.ok(&hub, "proposal.list", json!({}));
    assert_eq!(list["pending"], 1);
    assert_eq!(list["proposals"][0]["label"], "Two tracks");
    assert_eq!(list["proposals"][0]["applies"], true);
    assert_eq!(list["proposals"][0]["stale"], false);

    // A dry run accept reports the hash the accept will reach and changes nothing.
    let dry = c.ok(
        &hub,
        "proposal.resolve",
        json!({ "id": "p1", "action": "accept", "dry_run": true }),
    );
    assert_eq!(dry["state_hash"], h0);
    assert_eq!(c.ok(&hub, "state.hash", json!({}))["state_hash"], h0);

    let done = c.ok(
        &hub,
        "proposal.resolve",
        json!({ "id": "p1", "action": "accept" }),
    );
    assert_eq!(done["status"], "accepted");
    assert_eq!(done["undo_depth"], 1);
    assert_eq!(done["state_hash"], dry["state_hash_after"]);
    let hist = c.ok(&hub, "history.get", json!({}));
    assert_eq!(hist["total"], 1);
    assert_eq!(hist["entries"][0]["source"], "ai");
    assert_eq!(hist["entries"][0]["label"], "Two tracks");
    assert_eq!(hist["entries"][0]["command_count"], 2);
    assert_eq!(c.ok(&hub, "proposal.list", json!({}))["pending"], 0);
    assert_eq!(
        c.ok(&hub, "proposal.list", json!({ "status": "accepted" }))["proposals"][0]["id"],
        "p1"
    );
    // One undo step removes both tracks.
    c.ok(&hub, "undo", json!({}));
    assert_eq!(c.ok(&hub, "state.hash", json!({}))["state_hash"], h0);
    assert_eq!(
        c.code(
            &hub,
            "proposal.resolve",
            json!({ "id": "p1", "action": "accept" })
        ),
        "proposal_resolved"
    );
}

#[test]
fn reject_leaves_the_project_and_bad_input_is_refused() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    let h0 = c.ok(&hub, "state.hash", json!({}))["state_hash"].clone();
    c.ok(
        &hub,
        "proposal.post",
        json!({ "label": "x", "commands": [track(1)] }),
    );
    let r = c.ok(
        &hub,
        "proposal.resolve",
        json!({ "id": "p1", "action": "reject" }),
    );
    assert_eq!(r["status"], "rejected");
    assert_eq!(r["state_hash"], h0);
    assert_eq!(c.ok(&hub, "history.get", json!({}))["total"], 0);
    assert_eq!(
        c.code(
            &hub,
            "proposal.resolve",
            json!({ "id": "zz", "action": "accept" })
        ),
        "proposal_not_found"
    );
    assert_eq!(
        c.code(
            &hub,
            "proposal.resolve",
            json!({ "id": "p1", "action": "maybe" })
        ),
        "invalid_params"
    );
    assert_eq!(
        c.code(
            &hub,
            "proposal.post",
            json!({ "label": " ", "commands": [track(1)] })
        ),
        "invalid_params"
    );
    assert_eq!(
        c.code(
            &hub,
            "proposal.post",
            json!({ "label": "x", "commands": [] })
        ),
        "invalid_params"
    );
    // A proposal that cannot apply now is refused with the failing index.
    let bad = c.call(
        &hub,
        "proposal.post",
        json!({ "label": "bad", "commands": [track(9), track(9)] }),
    );
    assert_eq!(bad["error"]["data"]["index"], 1);
    assert_eq!(c.ok(&hub, "proposal.list", json!({}))["pending"], 0);
    // Dry run post stores nothing.
    let dry = c.ok(
        &hub,
        "proposal.post",
        json!({ "label": "x", "commands": [track(1)], "dry_run": true }),
    );
    assert_eq!(dry["dry_run"], true);
    assert_eq!(c.ok(&hub, "proposal.list", json!({}))["pending"], 0);
}

#[test]
fn a_proposal_that_went_stale_fails_without_effect_and_stays_pending() {
    let hub = Hub::new(4);
    let mut c = Client::new(&hub);
    c.ok(
        &hub,
        "proposal.post",
        json!({ "label": "T1", "commands": [track(1)] }),
    );
    // The person adds the same id by hand first.
    c.ok(&hub, "command.apply", json!({ "command": track(1) }));
    let h = c.ok(&hub, "state.hash", json!({}))["state_hash"].clone();
    let list = c.ok(&hub, "proposal.list", json!({}));
    assert_eq!(list["proposals"][0]["stale"], true);
    assert_eq!(list["proposals"][0]["applies"], false);
    let r = c.call(
        &hub,
        "proposal.resolve",
        json!({ "id": "p1", "action": "accept" }),
    );
    assert!(r.get("error").is_some());
    assert_eq!(c.ok(&hub, "state.hash", json!({}))["state_hash"], h);
    assert_eq!(c.ok(&hub, "proposal.list", json!({}))["pending"], 1);
}

#[test]
fn session_new_drops_proposals_and_other_clients_hear_the_count() {
    let hub = Hub::new(4);
    let mut a = Client::new(&hub);
    let b = Client::new(&hub);
    a.ok(
        &hub,
        "proposal.post",
        json!({ "label": "x", "commands": [track(1)] }),
    );
    let pushed: Value = serde_json::from_str(&b.rx.recv().expect("push")).unwrap();
    assert_eq!(pushed["method"], "proposals.changed");
    assert_eq!(pushed["params"]["pending"], 1);
    a.ok(&hub, "session.new", json!({ "name": "fresh" }));
    assert_eq!(a.ok(&hub, "proposal.list", json!({}))["pending"], 0);
}
