use serde::Serialize;

/// One JSON field of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FieldInfo {
    pub name: &'static str,
    /// Type as clients see it: `id` (32 hex digits), `tick`, `u8`, `u32`, `index`, `text`, or an enum with its values.
    pub ty: &'static str,
    /// True when `null` is accepted.
    pub nullable: bool,
}

/// One command: its stable name, a summary and its JSON fields (besides `command`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CommandInfo {
    pub name: &'static str,
    pub summary: &'static str,
    pub fields: &'static [FieldInfo],
}

const fn f(name: &'static str, ty: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        ty,
        nullable: false,
    }
}

const fn opt(name: &'static str, ty: &'static str) -> FieldInfo {
    FieldInfo {
        name,
        ty,
        nullable: true,
    }
}

const TRACK_KIND: &str = "instrument|audio";
const AUTOMATION_TARGET: &str = "object";
const EFFECT_KIND: &str = "stereo_tool|delay|crusher|gate|waveshaper|tidal";
const AUTOMATION_CURVE: &str = "hold|linear|smooth";

static COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "set_project_name",
        summary: "Rename the project.",
        fields: &[f("name", "text")],
    },
    CommandInfo {
        name: "set_tempo",
        summary: "Set the tempo (thousandths of a BPM) from a tick onward.",
        fields: &[f("tick", "tick"), f("mbpm", "u32")],
    },
    CommandInfo {
        name: "remove_tempo",
        summary: "Remove a tempo change (not the one at tick 0).",
        fields: &[f("tick", "tick")],
    },
    CommandInfo {
        name: "set_time_signature",
        summary: "Set the time signature from a tick onward.",
        fields: &[
            f("tick", "tick"),
            f("numerator", "u8"),
            f("denominator", "u8"),
        ],
    },
    CommandInfo {
        name: "remove_time_signature",
        summary: "Remove a signature change (not the one at tick 0).",
        fields: &[f("tick", "tick")],
    },
    CommandInfo {
        name: "set_key",
        summary: "Set the key (root 0 = C to 11 = B, and a scale) from a tick onward.",
        fields: &[
            f("tick", "tick"),
            f("root", "u8"),
            f("scale", "major|minor|dorian|mixolydian|pentatonic_major|pentatonic_minor|harmonic_minor"),
        ],
    },
    CommandInfo {
        name: "remove_key",
        summary: "Remove a key change; removing the last one leaves the project with no key.",
        fields: &[f("tick", "tick")],
    },
    CommandInfo {
        name: "set_marker",
        summary: "Put a named marker at a tick, or rename the marker already there.",
        fields: &[f("tick", "tick"), f("name", "text")],
    },
    CommandInfo {
        name: "remove_marker",
        summary: "Remove the marker at a tick.",
        fields: &[f("tick", "tick")],
    },
    CommandInfo {
        name: "set_region_loop",
        summary: "Repeat the first loop_length ticks of a music clip to fill the clip; 0 turns the loop off.",
        fields: &[f("region", "id"), f("loop_length", "tick")],
    },
    CommandInfo {
        name: "add_track",
        summary: "Add a track; index null appends.",
        fields: &[
            f("id", "id"),
            f("name", "text"),
            f("kind", TRACK_KIND),
            opt("index", "index"),
        ],
    },
    CommandInfo {
        name: "rename_track",
        summary: "Rename a track.",
        fields: &[f("track", "id"), f("name", "text")],
    },
    CommandInfo {
        name: "set_instrument",
        summary: "Set or clear the instrument slot of an instrument track.",
        fields: &[f("track", "id"), opt("instrument", "id")],
    },
    CommandInfo {
        name: "move_track",
        summary: "Move a track to a new position in the list.",
        fields: &[f("track", "id"), f("index", "index")],
    },
    CommandInfo {
        name: "delete_track",
        summary: "Delete a track with its clips and notes.",
        fields: &[f("track", "id")],
    },
    CommandInfo {
        name: "add_region",
        summary: "Add a clip (midi on instrument tracks, audio on audio tracks).",
        fields: &[
            f("id", "id"),
            f("track", "id"),
            f("start", "tick"),
            f("length", "tick"),
            f("kind", "midi|audio"),
        ],
    },
    CommandInfo {
        name: "add_audio_clip",
        summary: "Add an audio clip that plays an imported audio file (by content hash) on an audio track. Offset is the first source frame, gain is permille of unity.",
        fields: &[
            f("id", "id"),
            f("track", "id"),
            f("start", "tick"),
            f("length", "tick"),
            f("asset", "hash"),
            opt("offset", "u64"),
            opt("gain", "u16"),
        ],
    },
    CommandInfo {
        name: "set_clip_gain",
        summary: "Set the gain of an audio clip in permille of unity (1000 is unity, 0..2000).",
        fields: &[f("region", "id"), f("gain", "u16")],
    },
    CommandInfo {
        name: "set_clip_offset",
        summary: "Set the first source frame an audio clip plays, to trim its start.",
        fields: &[f("region", "id"), f("offset", "u64")],
    },
    CommandInfo {
        name: "move_region",
        summary: "Move a clip to a new start tick.",
        fields: &[f("region", "id"), f("start", "tick")],
    },
    CommandInfo {
        name: "resize_region",
        summary: "Change the length of a clip.",
        fields: &[f("region", "id"), f("length", "tick")],
    },
    CommandInfo {
        name: "delete_region",
        summary: "Delete a clip with its notes.",
        fields: &[f("region", "id")],
    },
    CommandInfo {
        name: "add_note",
        summary: "Add a note to a midi clip; start is relative to the clip.",
        fields: &[
            f("id", "id"),
            f("region", "id"),
            f("pitch", "u8"),
            f("velocity", "u8"),
            f("start", "tick"),
            f("duration", "tick"),
        ],
    },
    CommandInfo {
        name: "move_note",
        summary: "Move a note in time and pitch.",
        fields: &[
            f("region", "id"),
            f("note", "id"),
            f("start", "tick"),
            f("pitch", "u8"),
        ],
    },
    CommandInfo {
        name: "resize_note",
        summary: "Change the duration of a note.",
        fields: &[f("region", "id"), f("note", "id"), f("duration", "tick")],
    },
    CommandInfo {
        name: "set_note_velocity",
        summary: "Change the velocity of a note.",
        fields: &[f("region", "id"), f("note", "id"), f("velocity", "u8")],
    },
    CommandInfo {
        name: "delete_note",
        summary: "Delete a note.",
        fields: &[f("region", "id"), f("note", "id")],
    },
    CommandInfo {
        name: "move_region_to_track",
        summary: "Move a clip to another track of the same kind, keeping its start.",
        fields: &[f("region", "id"), f("track", "id")],
    },
    CommandInfo {
        name: "set_track_sound",
        summary: "Set or clear the catalogue sound id of an instrument track (built in voices are listed by sounds.list with kind builtin).",
        fields: &[f("track", "id"), opt("sound", "text")],
    },
    CommandInfo {
        name: "set_track_volume",
        summary: "Set the volume of a track in permille of unity (1000 is unity, 0..2000).",
        fields: &[f("track", "id"), f("volume", "u16")],
    },
    CommandInfo {
        name: "set_master_volume",
        summary: "Set the master volume in permille of unity (1000 is unity, 0..2000).",
        fields: &[f("volume", "u16")],
    },
    CommandInfo {
        name: "set_master_reverb",
        summary: "Set the master reverb room size and return level in permille (0..1000 each; a level of 0 turns it off).",
        fields: &[f("size", "u16"), f("mix", "u16")],
    },
    CommandInfo {
        name: "set_track_reverb_send",
        summary: "Set how much of a track feeds the master reverb in permille (0..1000).",
        fields: &[f("track", "id"), f("send", "u16")],
    },
    CommandInfo {
        name: "add_automation_point",
        summary: "Add a point to an automation lane (the lane appears with its first point). target is {kind: master_volume} or {kind: track_volume|track_pan|track_reverb_send, track}. value is in permille: volume 0..2000, pan -1000..1000, send 0..1000. curve (default linear) says how the value travels to the next point.",
        fields: &[
            f("target", AUTOMATION_TARGET),
            f("tick", "tick"),
            f("value", "i16"),
            opt("curve", AUTOMATION_CURVE),
        ],
    },
    CommandInfo {
        name: "move_automation_point",
        summary: "Move an automation point to a new tick and value; its curve is kept and the new tick must be free.",
        fields: &[
            f("target", AUTOMATION_TARGET),
            f("tick", "tick"),
            f("new_tick", "tick"),
            f("value", "i16"),
        ],
    },
    CommandInfo {
        name: "set_automation_curve",
        summary: "Change how the value travels from an automation point to the next one.",
        fields: &[
            f("target", AUTOMATION_TARGET),
            f("tick", "tick"),
            f("curve", AUTOMATION_CURVE),
        ],
    },
    CommandInfo {
        name: "delete_automation_point",
        summary: "Remove one automation point; removing the last one removes the lane.",
        fields: &[f("target", AUTOMATION_TARGET), f("tick", "tick")],
    },
    CommandInfo {
        name: "clear_automation_lane",
        summary: "Remove every point of an automation lane, as one edit.",
        fields: &[f("target", AUTOMATION_TARGET)],
    },
    CommandInfo {
        name: "set_track_pan",
        summary: "Set the pan of a track in permille (-1000 left, 0 center, 1000 right).",
        fields: &[f("track", "id"), f("pan", "i16")],
    },
    CommandInfo {
        name: "set_track_mute",
        summary: "Mute or unmute a track.",
        fields: &[f("track", "id"), f("mute", "bool")],
    },
    CommandInfo {
        name: "set_track_solo",
        summary: "Solo or unsolo a track; while any track is soloed the others are silent.",
        fields: &[f("track", "id"), f("solo", "bool")],
    },
    CommandInfo {
        name: "add_notes",
        summary: "Add many notes to one midi clip at once; each note has id, pitch, velocity, start (relative to the clip) and duration.",
        fields: &[f("region", "id"), f("notes", "notes")],
    },
    CommandInfo {
        name: "split_region",
        summary: "Cut a clip at an absolute tick. The right part gets new_region; note_ids holds one new id per note crossing the cut (in note id order).",
        fields: &[
            f("region", "id"),
            f("at", "tick"),
            f("new_region", "id"),
            f("note_ids", "ids"),
        ],
    },
    CommandInfo {
        name: "duplicate_region",
        summary: "Copy a clip to a start tick, on the same track or another (track null). note_ids holds one new id per note in the clip (in note id order).",
        fields: &[
            f("region", "id"),
            f("new_region", "id"),
            f("start", "tick"),
            opt("track", "id"),
            f("note_ids", "ids"),
        ],
    },
    CommandInfo {
        name: "transpose_notes",
        summary: "Shift pitches by semitones for the listed notes of a clip (notes null means all notes).",
        fields: &[f("region", "id"), f("semitones", "i16"), opt("notes", "ids")],
    },
    CommandInfo {
        name: "quantize_notes",
        summary: "Pull note starts toward a grid of grid ticks by strength percent (1..100) for the listed notes (null means all).",
        fields: &[
            f("region", "id"),
            f("grid", "tick"),
            f("strength", "u8"),
            opt("notes", "ids"),
        ],
    },
    CommandInfo {
        name: "move_notes",
        summary: "Move the listed notes of a clip by delta_tick ticks and delta_pitch semitones as one edit. Rejected whole if any note would leave the clip or 0..127.",
        fields: &[
            f("region", "id"),
            f("notes", "ids"),
            f("delta_tick", "tick"),
            f("delta_pitch", "i16"),
        ],
    },
    CommandInfo {
        name: "resize_notes",
        summary: "Add delta ticks (may be negative) to the duration of the listed notes. Rejected whole if any note would get shorter than 1 tick or end after the clip.",
        fields: &[f("region", "id"), f("notes", "ids"), f("delta", "tick")],
    },
    CommandInfo {
        name: "set_notes_velocity",
        summary: "Set velocity (1..127) of the listed notes: give value for one level, or ramp {from, to} to interpolate by note start. Exactly one of value and ramp.",
        fields: &[
            f("region", "id"),
            f("notes", "ids"),
            opt("value", "u8"),
            opt("ramp", "object"),
        ],
    },
    CommandInfo {
        name: "delete_notes",
        summary: "Remove the listed notes of a clip as one edit.",
        fields: &[f("region", "id"), f("notes", "ids")],
    },
    CommandInfo {
        name: "conform_notes_to_scale",
        summary: "Move notes that are outside the project key to the nearest in key pitch (the lower one on a tie); notes null means all notes of the clip. Needs a key (set_key).",
        fields: &[f("region", "id"), opt("notes", "ids")],
    },
    CommandInfo {
        name: "add_effect",
        summary: "Add an audio effect with default parameters to a chain. owner is {kind: master} or {kind: track, track} (instrument tracks only); kind is stereo_tool, delay, crusher, gate, waveshaper or tidal; index null appends. The chain runs after the track strip and before the master.",
        fields: &[
            f("id", "id"),
            f("owner", "object"),
            f("kind", EFFECT_KIND),
            opt("index", "index"),
        ],
    },
    CommandInfo {
        name: "remove_effect",
        summary: "Remove an audio effect from its chain.",
        fields: &[f("effect", "id")],
    },
    CommandInfo {
        name: "move_effect",
        summary: "Move an audio effect to another position of its chain.",
        fields: &[f("effect", "id"), f("index", "index")],
    },
    CommandInfo {
        name: "set_effect_bypass",
        summary: "Switch an audio effect off (bypass true) or on. A bypassed effect passes audio through unchanged.",
        fields: &[f("effect", "id"), f("bypass", "bool")],
    },
    CommandInfo {
        name: "set_effect_param",
        summary: "Set one parameter of an audio effect by name, as an integer in the range of that parameter. stereo_tool: volume 0..2000, pan -1000..1000, invert_left, invert_right, mono (0 or 1). delay: time_ms 1..2000, feedback 0..950, filter 0..1000, mix 0..1000. crusher: bits 1..16, rate_reduction 1..64, mix 0..1000. gate: threshold 0..1000, attack_ms 0..200, release_ms 1..2000. waveshaper: drive 0..1000, shape 0..1000, mix 0..1000. tidal: rate_centihz 1..2000, depth 0..1000, shape 0..1000, pan_depth 0..1000.",
        fields: &[f("effect", "id"), f("param", "text"), f("value", "i32")],
    },
];

/// Every command with its name and JSON fields, for the CLI and MCP layers.
pub fn describe() -> &'static [CommandInfo] {
    COMMANDS
}
