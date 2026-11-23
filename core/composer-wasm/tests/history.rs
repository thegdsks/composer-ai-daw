//! History, step jumps, open groups, commands.enabled and describe metadata over the ed_* ABI (D20, D32, D33).

use composer_wasm::{ffi, EditorSession};
use serde_json::Value;

const ADD_TRACK: &str =
    r#"{"command":"add_track","id":"a","name":"lead","kind":"instrument","index":null}"#;
const SET_INSTRUMENT: &str = r#"{"command":"set_instrument","track":"a","instrument":"2"}"#;
const ADD_REGION: &str =
    r#"{"command":"add_region","id":"b","track":"a","start":0,"length":3840,"kind":"midi"}"#;

unsafe fn json_of(h: *mut EditorSession, p: *const u8) -> Value {
    assert!(!p.is_null());
    serde_json::from_slice(std::slice::from_raw_parts(p, ffi::ed_result_len(h))).unwrap()
}

unsafe fn apply(h: *mut EditorSession, json: &str) -> Value {
    let p = ffi::ed_apply_json(h, json.as_ptr(), json.len());
    json_of(h, p)
}

unsafe fn call(
    h: *mut EditorSession,
    f: unsafe extern "C" fn(*mut EditorSession, *const u8, usize) -> *const u8,
    json: &str,
) -> Value {
    let p = f(h, json.as_ptr(), json.len());
    json_of(h, p)
}

unsafe fn new() -> *mut EditorSession {
    let h = ffi::ed_new(b"song".as_ptr(), 4);
    assert!(!h.is_null());
    h
}

fn hex(h: u64) -> String {
    format!("{h:016x}")
}

#[test]
fn history_pages_with_cursor_and_caps() {
    unsafe {
        let h = new();
        apply(h, ADD_TRACK);
        apply(h, SET_INSTRUMENT);
        apply(h, ADD_REGION);
        ffi::ed_undo_steps(h, 1);

        let v = call(h, ffi::ed_history_json, "");
        let r = &v["result"];
        assert_eq!(r["total"], 3);
        assert_eq!(r["cursor"], 2);
        assert_eq!(r["entries"].as_array().unwrap().len(), 3);
        assert_eq!(r["entries"][0]["index"], 0);
        assert_eq!(r["entries"][0]["source"], "user");
        assert_eq!(r["entries"][0]["command_count"], 1);

        let v = call(h, ffi::ed_history_json, r#"{"limit":1,"offset":1}"#);
        let e = v["result"]["entries"].as_array().unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0]["index"], 1);
        assert_eq!(v["result"]["offset"], 1);

        let v = call(h, ffi::ed_history_json, r#"{"nope":1}"#);
        assert_eq!(v["error"]["data"]["code"], "invalid_params");
        ffi::ed_free(h);
    }
}

#[test]
fn undo_and_redo_steps_restore_recorded_hashes() {
    unsafe {
        let h = new();
        let mut hashes = vec![ffi::ed_state_hash(h)];
        for c in [ADD_TRACK, SET_INSTRUMENT, ADD_REGION] {
            apply(h, c);
            hashes.push(ffi::ed_state_hash(h));
        }
        let v = json_of(h, ffi::ed_undo_steps(h, 2));
        assert_eq!(v["result"]["steps_applied"], 2);
        assert_eq!(v["result"]["applied"], true);
        assert_eq!(ffi::ed_state_hash(h), hashes[1]);
        assert_eq!(v["result"]["state_hash"], hex(hashes[1]));

        // Asking for more than exists moves what is available.
        let v = json_of(h, ffi::ed_redo_steps(h, 9));
        assert_eq!(v["result"]["steps_applied"], 2);
        assert_eq!(ffi::ed_state_hash(h), hashes[3]);
        let v = json_of(h, ffi::ed_redo_steps(h, 1));
        assert_eq!(v["result"]["steps_applied"], 0);
        assert_eq!(v["result"]["applied"], false);

        let v = json_of(h, ffi::ed_undo_steps(h, 0));
        assert_eq!(v["error"]["data"]["code"], "invalid_params");
        let v = json_of(h, ffi::ed_undo_steps(h, 99));
        assert_eq!(v["result"]["steps_applied"], 3);
        assert_eq!(ffi::ed_state_hash(h), hashes[0]);
        ffi::ed_free(h);
    }
}

#[test]
fn group_commit_is_one_step_with_source_and_count() {
    unsafe {
        let h = new();
        let v = call(
            h,
            ffi::ed_group_begin_json,
            r#"{"label":"idea","source":"ai"}"#,
        );
        let token = v["result"]["token"].as_str().unwrap().to_string();
        assert_eq!(token.len(), 32);
        apply(h, ADD_TRACK);
        apply(h, SET_INSTRUMENT);

        // Open group: second begin and undo are rejected.
        let v = call(h, ffi::ed_group_begin_json, r#"{"label":"again"}"#);
        assert_eq!(v["error"]["data"]["code"], "group_already_open");
        let v = json_of(h, ffi::ed_undo_steps(h, 1));
        assert_eq!(v["error"]["data"]["code"], "group_already_open");

        let body = format!(r#"{{"token":"{token}"}}"#);
        let v = call(h, ffi::ed_group_commit_json, &body);
        assert_eq!(v["result"]["undo_depth"], 1);
        assert_eq!(v["result"]["undo_label"], "idea");

        let v = call(h, ffi::ed_history_json, "");
        assert_eq!(v["result"]["entries"][0]["source"], "ai");
        assert_eq!(v["result"]["entries"][0]["command_count"], 2);

        // Closed group: the token no longer matches anything.
        let v = call(h, ffi::ed_group_commit_json, &body);
        assert_eq!(v["error"]["data"]["code"], "no_open_group");
        ffi::ed_free(h);
    }
}

#[test]
fn group_rollback_restores_hash_and_wrong_token_is_rejected() {
    unsafe {
        let h = new();
        apply(h, ADD_TRACK);
        let before = ffi::ed_state_hash(h);
        let v = call(h, ffi::ed_group_begin_json, r#"{"label":"trial"}"#);
        let token = v["result"]["token"].as_str().unwrap().to_string();
        apply(h, SET_INSTRUMENT);
        assert_ne!(ffi::ed_state_hash(h), before);

        let v = call(h, ffi::ed_group_rollback_json, r#"{"token":"ff"}"#);
        assert_eq!(v["error"]["data"]["code"], "group_token_mismatch");
        assert_ne!(ffi::ed_state_hash(h), before);

        let v = call(
            h,
            ffi::ed_group_rollback_json,
            &format!(r#"{{"token":"{token}"}}"#),
        );
        assert_eq!(v["result"]["undo_depth"], 1);
        assert_eq!(ffi::ed_state_hash(h), before);

        // Empty group adds no step; a new group gets a fresh token.
        let v = call(h, ffi::ed_group_begin_json, r#"{"label":"empty"}"#);
        let t2 = v["result"]["token"].as_str().unwrap().to_string();
        assert_ne!(t2, token);
        call(
            h,
            ffi::ed_group_commit_json,
            &format!(r#"{{"token":"{t2}"}}"#),
        );
        let v = call(h, ffi::ed_history_json, "");
        assert_eq!(v["result"]["total"], 1);

        let v = call(h, ffi::ed_group_begin_json, r#"{"label":" "}"#);
        assert_eq!(v["error"]["data"]["code"], "invalid_params");
        let v = call(
            h,
            ffi::ed_group_begin_json,
            r#"{"label":"x","source":"bot"}"#,
        );
        assert_eq!(v["error"]["data"]["code"], "invalid_params");
        ffi::ed_free(h);
    }
}

#[test]
fn load_drops_an_open_group() {
    unsafe {
        let h = new();
        let v = call(h, ffi::ed_group_begin_json, r#"{"label":"t"}"#);
        let token = v["result"]["token"].as_str().unwrap().to_string();
        let state = json_of(h, ffi::ed_state_json(h));
        let project = state["result"]["project"].to_string();
        call(h, ffi::ed_load_json, &project);
        let v = call(
            h,
            ffi::ed_group_commit_json,
            &format!(r#"{{"token":"{token}"}}"#),
        );
        assert_eq!(v["error"]["data"]["code"], "no_open_group");
        ffi::ed_free(h);
    }
}

#[test]
fn enabled_reports_every_command_and_follows_the_project() {
    unsafe {
        let h = new();
        let v = call(h, ffi::ed_enabled_json, "");
        let list = v["result"]["commands"].as_array().unwrap();
        assert!(!list.is_empty());
        let entry = |list: &Vec<Value>, name: &str| -> Value {
            list.iter().find(|c| c["name"] == name).unwrap().clone()
        };
        let del = entry(list, "delete_track");
        assert_eq!(del["enabled"], false);
        assert!(del["reason"].is_string());
        let before = ffi::ed_state_hash(h);

        apply(h, ADD_TRACK);
        let v = call(h, ffi::ed_enabled_json, r#"{"context":{"track":"a"}}"#);
        let list = v["result"]["commands"].as_array().unwrap();
        assert_eq!(entry(list, "delete_track")["enabled"], true);
        assert_ne!(ffi::ed_state_hash(h), before);

        let v = call(h, ffi::ed_enabled_json, r#"{"bogus":1}"#);
        assert_eq!(v["error"]["data"]["code"], "invalid_params");
        ffi::ed_free(h);
    }
}

#[test]
fn describe_entries_carry_palette_metadata() {
    unsafe {
        let h = new();
        let v = json_of(h, ffi::ed_describe_json(h));
        let list = v["result"]["commands"].as_array().unwrap();
        assert!(!list.is_empty());
        for c in list {
            assert!(c["verb"].is_string(), "{c}");
            assert!(c["label"].is_string(), "{c}");
            assert!(!c["keywords"].as_array().unwrap().is_empty(), "{c}");
        }
        ffi::ed_free(h);
    }
}
