//! Serializable command API shared by every client (UI, CLI, MCP).
//!
//! A [`Command`] is a semantic, JSON serializable edit. `Command::plan`
//! validates it and compiles it to invertible [`composer_model::Change`]
//! records; [`Editor`] applies them atomically and keeps undo and redo, with
//! transaction groups that undo as one step. [`describe`] lists every command
//! with its name and JSON fields.
//!
//! Commands (JSON `command` value): `set_project_name`, `set_tempo`,
//! `remove_tempo`, `set_time_signature`, `remove_time_signature`, `set_key`, `remove_key`, `add_track`,
//! `rename_track`, `set_instrument`, `set_track_sound`, `move_track`, `delete_track`, `add_region`,
//! `move_region`, `resize_region`, `delete_region`, `add_note`, `move_note`,
//! `resize_note`, `set_note_velocity`, `delete_note`, `move_region_to_track`, `set_track_volume`, `set_track_pan`,
//! `set_track_mute`, `set_track_solo`, `add_notes`, `split_region`,
//! `duplicate_region`, `transpose_notes`, `quantize_notes`, `move_notes`,
//! `resize_notes`, `set_notes_velocity`, `delete_notes`, `conform_notes_to_scale`, `add_audio_clip`,
//! `set_clip_gain`, `set_clip_offset` (D40), and the automation
//! commands `add_automation_point`, `move_automation_point`, `set_automation_curve`,
//! `delete_automation_point`, `clear_automation_lane` (D36).

mod command;
mod describe;
mod editor;
mod error;
mod meta;
mod read;

pub use command::{Command, NoteSpec, VelocityRamp, MAX_BULK_NOTES};
pub use describe::{describe, CommandInfo, FieldInfo};
pub use editor::{Editor, HistoryEntry, HistoryPage, Source, Step};
pub use error::CommandError;
pub use meta::{command_meta, commands_enabled, CommandMeta, EnabledContext, EnabledEntry};
pub use read::{
    project_tree, NoteNode, NoteRef, ProjectTree, RegionNode, Selection, TrackNode, TreeOptions,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_audio;
#[cfg(test)]
mod tests_automation;
#[cfg(test)]
mod tests_bulk;
#[cfg(test)]
mod tests_conform;
#[cfg(test)]
mod tests_effects;
#[cfg(test)]
mod tests_hash_cache;
#[cfg(test)]
mod tests_history;
#[cfg(test)]
mod tests_read;
