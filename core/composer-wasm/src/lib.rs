//! C ABI glue that runs the engine inside a browser AudioWorklet.
//!
//! No wasm-bindgen: a worklet instantiates the module with a tiny hand-written loader.
//! All `unsafe` in the workspace lives in [`ffi`]; the engine crates forbid it.
//!
//! # Blob format (version 2, little endian; version 1 is still read, see D19)
//!
//! ```text
//! header (32 bytes)
//!   u32 magic = 0x57504D43 (bytes "CMPW")
//!   u32 version = 2
//!   u32 sample_rate
//!   u32 voices_per_track
//!   u32 track_count
//!   u32 event_count
//!   u64 length_frames      informational: where a full offline render ends (last off + tail)
//! tracks, track_count x 16 bytes (v1: 12 bytes, the first 12 of these with bytes 1..4 zero)
//!   u8 waveform (0 sine, 1 saw, 2 to 9 built in voices), u8 flags (bit0 mute, bit1 solo), u16 volume permille,
//!   f32 gain, f32 pan, i16 pan permille, u8[2] zero
//!   effective gain = gain * volume/1000, zero when muted or when another track is soloed
//! events, event_count x 16 bytes, in any order (the engine sorts them)
//!   u64 frame, u32 track, u8 kind (0 off, 1 on), u8 pitch, u8 velocity, u8 zero
//! ```
//!
//! # Blob v3 (D34)
//!
//! Written only when a track plays a sample bank. Same header and events as v2 with version 3;
//! the last two bytes of a track record hold the bank index plus one (0 means synth). After the
//! events: `u32 bank_count`, then per bank `u32 sample_count, u32 zone_count`, the samples
//! (`u32 rate, u32 channels, u32 frames`, then frames x channels `f32`) and the zones (48 byte
//! records: `u32 sample, u8 key_lo, key_hi, vel_lo, vel_hi, root, loop_mode (0 none, 1
//! continuous, 2 sustain, 3 one shot), i16 scale_cents, i32 tune_cents, u32 loop_start,
//! u32 loop_end, f32 attack, decay, sustain, release, gain, pan`). Counts are checked against
//! the bytes left, so decode never allocates more than the blob holds.
//!
//! # Call sequence
//!
//! `cw_alloc` a buffer, copy the blob in, `cw_create`, then `cw_dealloc` the blob.
//! Per quantum: `cw_process(h, 128)`, then read `cw_left`/`cw_right` (128 f32 each, owned by
//! the engine handle, valid until `cw_free`). Memory growth can detach JS views, so
//! re-create typed arrays over `memory.buffer` when it changes.
//! `cw_note` queues a live note on or off for the next block (preallocated queue).
//!
//! # Editor surface (`ed_*`)
//!
//! A second handle type wraps the command editor so the browser needs no daemon. JSON shapes
//! mirror composerd: a reply is `{"result":{...}}` or `{"error":{"code","message","data":{"code"}}}`
//! with the same result fields (`state_hash` as 16 hex digits, `undo_depth`, `redo_depth`,
//! `undo_label`). Every `ed_*` call that returns bytes fills one result buffer owned by the
//! handle: read it with `ed_result_ptr` and `ed_result_len` before the next call on that
//! handle, then copy it; the next call or `ed_free` invalidates it. A null return means a bad
//! argument (null handle, invalid UTF-8, bad name or zero sample rate).
#![deny(unsafe_code)]

mod blob;
mod demo;
mod editor_session;
mod proposals;
mod session;

pub use blob::{
    compile_project, compile_project_with_banks, compile_project_with_bus,
    compile_project_with_sounds, decode, encode, sound_bank_key, BlobError, Decoded, TrackSpec,
};
pub use demo::demo_blob;
pub use editor_session::EditorSession;
pub use session::Session;

#[allow(unsafe_code)]
pub mod ffi;
