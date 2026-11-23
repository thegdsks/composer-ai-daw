//! The C ABI. Handles are opaque pointers from `cw_create`; JS must not use one after `cw_free`.
//! Every function tolerates a null handle (returns 0 or does nothing).

use crate::demo::demo_blob;
use crate::session::Session;

/// Allocates `len` bytes in module memory for the host to fill. Free with [`cw_dealloc`].
#[no_mangle]
pub extern "C" fn cw_alloc(len: usize) -> *mut u8 {
    let b = vec![0u8; len].into_boxed_slice();
    Box::into_raw(b) as *mut u8
}

/// Frees a buffer from [`cw_alloc`] or [`cw_demo_blob`].
///
/// # Safety
/// `ptr` and `len` must be exactly a pair returned earlier and not freed yet.
#[no_mangle]
pub unsafe extern "C" fn cw_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the caller guarantees ptr/len came from a boxed slice of this length.
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}

/// Builds the demo project blob and writes its length to `len_out`. Free with [`cw_dealloc`].
///
/// # Safety
/// `len_out` must be a valid, aligned pointer to a `u32`.
#[no_mangle]
pub unsafe extern "C" fn cw_demo_blob(sample_rate: u32, len_out: *mut u32) -> *mut u8 {
    let v = demo_blob(sample_rate, 0.25).into_boxed_slice();
    // SAFETY: caller guarantees len_out is valid for a u32 write.
    *len_out = v.len() as u32;
    Box::into_raw(v) as *mut u8
}

/// Creates an engine from a blob. Returns null when the blob is invalid.
///
/// # Safety
/// `ptr` must be valid for reads of `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cw_create(ptr: *const u8, len: usize) -> *mut Session {
    // SAFETY: the caller guarantees ptr is readable for len bytes.
    let bytes = std::slice::from_raw_parts(ptr, len);
    match Session::from_blob(bytes) {
        Ok(s) => Box::into_raw(Box::new(s)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Destroys an engine.
///
/// # Safety
/// `h` must come from [`cw_create`] and not be freed already.
#[no_mangle]
pub unsafe extern "C" fn cw_free(h: *mut Session) {
    if !h.is_null() {
        // SAFETY: h was created by Box::into_raw in cw_create and is freed once.
        drop(Box::from_raw(h));
    }
}

/// Renders up to 128 frames; returns how many were rendered.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_process(h: *mut Session, frames: u32) -> u32 {
    // SAFETY: the caller guarantees a live handle; we hold the only reference during the call.
    h.as_mut().map_or(0, |s| s.process(frames as usize) as u32)
}

/// Pointer to the 128 left samples of the last render.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_left(h: *mut Session) -> *const f32 {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(std::ptr::null(), |s| s.left.as_ptr())
}

/// Pointer to the 128 right samples of the last render.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_right(h: *mut Session) -> *const f32 {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(std::ptr::null(), |s| s.right.as_ptr())
}

/// Frames rendered so far.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_position(h: *const Session) -> u64 {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(0, Session::position)
}

/// Frames whose master sum went past full scale.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_clipped(h: *const Session) -> u64 {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(0, Session::clipped_frames)
}

/// Copies the per track peaks of the last block (blob track order) into `out`, at most `max`
/// values, and returns the number of tracks (which may exceed `max`). Allocation free.
///
/// # Safety
/// `h` must be null or a live handle; `out` must be null or valid for `max` f32 writes.
#[no_mangle]
pub unsafe extern "C" fn cw_track_peaks(h: *const Session, out: *mut f32, max: u32) -> u32 {
    // SAFETY: live handle per the contract.
    let Some(s) = h.as_ref() else {
        return 0;
    };
    let peaks = s.track_peaks();
    if !out.is_null() {
        let n = peaks.len().min(max as usize);
        // SAFETY: the caller guarantees `out` holds `max` floats and n <= max.
        std::ptr::copy_nonoverlapping(peaks.as_ptr(), out, n);
    }
    peaks.len() as u32
}

/// Sets the master soft clip ceiling (linear, 0.1 to 1.0); 0 turns it off (the default).
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_set_ceiling(h: *mut Session, ceiling: f32) {
    // SAFETY: the caller guarantees a live handle; we hold the only reference during the call.
    if let Some(s) = h.as_mut() {
        s.set_ceiling(ceiling);
    }
}

/// Where an offline render of the loaded project ends (0 when unknown).
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_length(h: *const Session) -> u64 {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(0, |s| s.length_frames)
}

/// Queues a live note on or off for the next block. Returns 1 when queued, 0 when dropped.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn cw_note(
    h: *mut Session,
    track: u32,
    pitch: u8,
    velocity: u8,
    on: u32,
) -> u32 {
    // SAFETY: live handle per the contract; exclusive for the call.
    h.as_mut()
        .map_or(0, |s| u32::from(s.note(track, pitch, velocity, on != 0)))
}

// ---- Editor surface. Result buffer rules are in the crate docs. ----

use crate::editor_session::EditorSession;

/// Creates an editor with an empty project. Null when the name is invalid.
///
/// # Safety
/// `name_ptr` must be valid for reads of `name_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_new(name_ptr: *const u8, name_len: usize) -> *mut EditorSession {
    // SAFETY: the caller guarantees name_ptr is readable for name_len bytes.
    let bytes = std::slice::from_raw_parts(name_ptr, name_len);
    std::str::from_utf8(bytes)
        .ok()
        .and_then(EditorSession::new)
        .map_or(std::ptr::null_mut(), |s| Box::into_raw(Box::new(s)))
}

/// Destroys an editor and its result buffer.
///
/// # Safety
/// `h` must come from [`ed_new`] and not be freed already.
#[no_mangle]
pub unsafe extern "C" fn ed_free(h: *mut EditorSession) {
    if !h.is_null() {
        // SAFETY: h was created by Box::into_raw in ed_new and is freed once.
        drop(Box::from_raw(h));
    }
}

/// Pointer to the latest result buffer (see crate docs for its lifetime).
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_result_ptr(h: *const EditorSession) -> *const u8 {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(std::ptr::null(), |s| s.result().as_ptr())
}

/// Length in bytes of the latest result buffer.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_result_len(h: *const EditorSession) -> usize {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(0, |s| s.result().len())
}

/// Runs `f` on the handle; returns the result pointer, or null for a null handle.
fn call(h: *mut EditorSession, f: impl FnOnce(&mut EditorSession)) -> *const u8 {
    // SAFETY: the ed_* callers guarantee h is null or live and not used concurrently.
    match unsafe { h.as_mut() } {
        Some(s) => {
            f(s);
            s.result().as_ptr()
        }
        None => std::ptr::null(),
    }
}

/// Turns the change feed (D25) on for `max_changes` above zero (feeds with more changes are
/// reported as `full`) or off for zero. Off by default, so results keep their old shape.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_set_feed(h: *mut EditorSession, max_changes: usize) {
    // SAFETY: live handle per the contract; exclusive for the call.
    if let Some(s) = h.as_mut() {
        s.set_feed(max_changes);
    }
}

/// Applies one command object. Result: new summary, or the stable error code.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_apply_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    // SAFETY: readable for len bytes per the contract.
    let input = std::slice::from_raw_parts(ptr, len);
    call(h, |s| s.apply(input))
}

/// Applies `{"label","commands":[...]}` atomically as one undo step.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_group_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    // SAFETY: readable for len bytes per the contract.
    let input = std::slice::from_raw_parts(ptr, len);
    call(h, |s| s.group(input))
}

/// Replaces the project from JSON (reindexed, fully validated, history cleared).
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_load_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    // SAFETY: readable for len bytes per the contract.
    let input = std::slice::from_raw_parts(ptr, len);
    call(h, |s| s.load(input))
}

/// `transport.get` (D21): playhead and loop, metronome, count in, follow.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_transport_get_json(h: *mut EditorSession) -> *const u8 {
    call(h, EditorSession::transport_get)
}

/// `transport.set` (D21): any of tick, loop, metronome, count_in_bars, follow, dry_run.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_transport_set_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    // SAFETY: readable for len bytes per the contract.
    let input = std::slice::from_raw_parts(ptr, len);
    call(h, |s| s.transport_set(input))
}

/// `theory.scale` (D23): scale pitch classes for `{"root","scale"}`.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_scale_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    // SAFETY: readable for len bytes per the contract.
    let input = std::slice::from_raw_parts(ptr, len);
    call(h, |s| s.scale_info(input))
}

/// `theory.chords` (D23b): diatonic triads and sevenths for `{"key":{"root","scale"}}`.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_chords_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    // SAFETY: readable for len bytes per the contract.
    let input = std::slice::from_raw_parts(ptr, len);
    call(h, |s| s.chords_info(input))
}

/// `theory.pitch` (D23b): in key flag, scale degree and nearest in key pitch for
/// `{"root","scale","pitch"}`.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_pitch_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    // SAFETY: readable for len bytes per the contract.
    let input = std::slice::from_raw_parts(ptr, len);
    call(h, |s| s.pitch_info(input))
}

/// Undoes the latest step.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_undo(h: *mut EditorSession) -> *const u8 {
    call(h, EditorSession::undo)
}

/// Redoes the latest undone step.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_redo(h: *mut EditorSession) -> *const u8 {
    call(h, EditorSession::redo)
}

/// Undoes up to `steps` steps (at least 1); result has `steps_applied`.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_undo_steps(h: *mut EditorSession, steps: u32) -> *const u8 {
    call(h, |s| s.jump(true, steps))
}

/// Redoes up to `steps` steps (at least 1); result has `steps_applied`.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_redo_steps(h: *mut EditorSession, steps: u32) -> *const u8 {
    call(h, |s| s.jump(false, steps))
}

/// Input slice that tolerates a null pointer when the length is zero.
///
/// # Safety
/// `ptr` is readable for `len` bytes unless `len` is 0.
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        // SAFETY: readable for len bytes per the contract.
        std::slice::from_raw_parts(ptr, len)
    }
}

/// `history.get` page from `{limit?, offset?}` (zero length means defaults).
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes (may be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn ed_history_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.history(input))
}

/// Opens the single group from `{label, source?}`; the result carries the token.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_group_begin_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.group_begin(input))
}

/// Commits the open group from `{token}` as one undo step.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_group_commit_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.group_close(input, true))
}

/// Rolls the open group back from `{token}`, restoring the state hash.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_group_rollback_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.group_close(input, false))
}

/// `commands.enabled` from `{context?}` (zero length means no context).
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes (may be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn ed_enabled_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.enabled(input))
}

/// Full project JSON plus its hash.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_state_json(h: *mut EditorSession) -> *const u8 {
    call(h, EditorSession::state)
}

/// The command catalogue (same as `commands.describe`).
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_describe_json(h: *mut EditorSession) -> *const u8 {
    call(h, EditorSession::describe)
}

/// The project state hash (0 for a null handle).
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_state_hash(h: *const EditorSession) -> u64 {
    // SAFETY: live handle per the contract.
    h.as_ref().map_or(0, EditorSession::state_hash)
}

/// Compiles the project to engine blob v1 into the result buffer. Null when the rate is 0.
///
/// # Safety
/// `h` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn ed_engine_blob(h: *mut EditorSession, sample_rate: u32) -> *const u8 {
    // SAFETY: live handle per the contract; exclusive for the call.
    h.as_mut().map_or(std::ptr::null(), |s| {
        if s.engine_blob(sample_rate) {
            s.result().as_ptr()
        } else {
            std::ptr::null()
        }
    })
}

/// Loads a SoundFont 2 file as the sample bank of catalogue sound `id` (D37). Returns 0 on
/// success, 1 for a bad id, 2 for a file the sampler refuses, 3 for a null handle.
///
/// # Safety
/// `h` is null or live; both pointers are readable for their lengths.
#[no_mangle]
pub unsafe extern "C" fn ed_sound_bank_load(
    h: *mut EditorSession,
    id_ptr: *const u8,
    id_len: usize,
    ptr: *const u8,
    len: usize,
) -> u32 {
    // SAFETY: readable for the given lengths per the contract.
    let id = std::slice::from_raw_parts(id_ptr, id_len);
    let data = std::slice::from_raw_parts(ptr, len);
    let Some(s) = h.as_mut() else { return 3 };
    let Ok(id) = std::str::from_utf8(id) else {
        return 1;
    };
    match s.load_sound_bank(id, data) {
        Ok(()) => 0,
        Err("sound.bad_id") => 1,
        Err(_) => 2,
    }
}

/// `proposal.post` (D27) from `{label, commands, note?, author?, dry_run?}`.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_proposal_post_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.proposal_post(input))
}

/// `proposal.list` (D27) from `{status?}` (zero length means pending).
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes (may be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn ed_proposal_list_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.proposal_list(input))
}

/// `proposal.resolve` (D27) from `{id, action, dry_run?}`; accept applies one group with source ai.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_proposal_resolve_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.proposal_resolve(input))
}

/// `templates.list` (D28): the bundled starter templates.
///
/// # Safety
/// `h` is null or live.
#[no_mangle]
pub unsafe extern "C" fn ed_templates_list_json(h: *mut EditorSession) -> *const u8 {
    call(h, |s| s.templates_list())
}

/// `recipes.list` (D28): recipes with their parameters.
///
/// # Safety
/// `h` is null or live.
#[no_mangle]
pub unsafe extern "C" fn ed_recipes_list_json(h: *mut EditorSession) -> *const u8 {
    call(h, |s| s.recipes_list())
}

/// `template.apply` (D28) from `{template, seed?, dry_run?}`; one group with source template.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_template_apply_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.template_apply(input))
}

/// `recipe.plan` (D28) from `{recipe, params?, scope?, seed?}`; returns commands, applies nothing.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_recipe_plan_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.recipe_plan(input))
}

/// `recipe.apply` (D28) from `{recipe, params?, scope?, seed?, dry_run?}`; one group with source recipe.
///
/// # Safety
/// `h` is null or live; `ptr` is readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn ed_recipe_apply_json(
    h: *mut EditorSession,
    ptr: *const u8,
    len: usize,
) -> *const u8 {
    let input = bytes(ptr, len);
    call(h, |s| s.recipe_apply(input))
}
