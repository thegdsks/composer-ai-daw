//! Proposals channel over the ed_* ABI (D27): same shapes and rules as composerd.

use composer_wasm::{ffi, EditorSession};
use serde_json::{json, Value};

type Call = unsafe extern "C" fn(*mut EditorSession, *const u8, usize) -> *const u8;

unsafe fn call(h: *mut EditorSession, f: Call, v: Value) -> Value {
    let s = v.to_string();
    let p = f(h, s.as_ptr(), s.len());
    assert!(!p.is_null());
    serde_json::from_slice(std::slice::from_raw_parts(p, ffi::ed_result_len(h))).unwrap()
}

fn track(n: u32) -> Value {
    json!({ "command": "add_track", "id": n.to_string(), "name": format!("T{n}"), "kind": "instrument", "index": null })
}

fn hex(h: u64) -> String {
    format!("{h:016x}")
}

#[test]
fn post_list_accept_reject_match_the_daemon_rules() {
    unsafe {
        let h = ffi::ed_new(b"song".as_ptr(), 4);
        let h0 = hex(ffi::ed_state_hash(h));
        let posted = call(
            h,
            ffi::ed_proposal_post_json,
            json!({ "label": "Two", "commands": [track(1), track(2)], "author": "bot" }),
        );
        let r = &posted["result"];
        assert_eq!(r["id"], "p1");
        assert_eq!(r["status"], "pending");
        assert_eq!(r["change_count"], 2);
        assert_eq!(hex(ffi::ed_state_hash(h)), h0, "post changes nothing");

        let l = call(h, ffi::ed_proposal_list_json, json!({}));
        assert_eq!(l["result"]["pending"], 1);
        assert_eq!(l["result"]["proposals"][0]["applies"], true);

        let dry = call(
            h,
            ffi::ed_proposal_resolve_json,
            json!({ "id": "p1", "action": "accept", "dry_run": true }),
        );
        assert_eq!(hex(ffi::ed_state_hash(h)), h0);
        let done = call(
            h,
            ffi::ed_proposal_resolve_json,
            json!({ "id": "p1", "action": "accept" }),
        );
        assert_eq!(done["result"]["status"], "accepted");
        assert_eq!(done["result"]["undo_depth"], 1);
        assert_eq!(
            done["result"]["state_hash"],
            dry["result"]["state_hash_after"]
        );
        let hist = call(h, ffi::ed_history_json, json!({}));
        assert_eq!(hist["result"]["entries"][0]["source"], "ai");
        assert_eq!(hist["result"]["entries"][0]["label"], "Two");

        let again = call(
            h,
            ffi::ed_proposal_resolve_json,
            json!({ "id": "p1", "action": "accept" }),
        );
        assert_eq!(again["error"]["data"]["code"], "proposal_resolved");

        call(
            h,
            ffi::ed_proposal_post_json,
            json!({ "label": "Three", "commands": [track(3)] }),
        );
        let after_accept = hex(ffi::ed_state_hash(h));
        let rej = call(
            h,
            ffi::ed_proposal_resolve_json,
            json!({ "id": "p2", "action": "reject" }),
        );
        assert_eq!(rej["result"]["status"], "rejected");
        assert_eq!(hex(ffi::ed_state_hash(h)), after_accept);

        // Stale: id 1 already exists, so the plan fails up front with its index.
        let bad = call(
            h,
            ffi::ed_proposal_post_json,
            json!({ "label": "dup", "commands": [track(1)] }),
        );
        assert!(bad["error"].is_object());
        let unknown = call(
            h,
            ffi::ed_proposal_resolve_json,
            json!({ "id": "p9", "action": "accept" }),
        );
        assert_eq!(unknown["error"]["data"]["code"], "proposal_not_found");
        ffi::ed_free(h);
    }
}
