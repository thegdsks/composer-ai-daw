//! Palette metadata and availability for described commands (D33).

use crate::describe::describe;
use composer_model::{Id, Project, RegionContent, TrackKind};
use serde::Serialize;

/// Verb, plain label and search keywords of one command.
struct Meta {
    name: &'static str,
    verb: &'static str,
    label: &'static str,
    keywords: &'static [&'static str],
}

const fn m(
    name: &'static str,
    verb: &'static str,
    label: &'static str,
    keywords: &'static [&'static str],
) -> Meta {
    Meta {
        name,
        verb,
        label,
        keywords,
    }
}

static META: &[Meta] = &[
    m(
        "set_project_name",
        "change",
        "Rename song",
        &["title", "name", "project"],
    ),
    m(
        "set_tempo",
        "change",
        "Change tempo",
        &["bpm", "speed", "faster", "slower"],
    ),
    m(
        "remove_tempo",
        "remove",
        "Remove tempo change",
        &["bpm", "speed", "delete"],
    ),
    m(
        "set_time_signature",
        "change",
        "Change time signature",
        &["meter", "beats", "waltz"],
    ),
    m(
        "remove_time_signature",
        "remove",
        "Remove time signature change",
        &["meter", "beats", "delete"],
    ),
    m(
        "set_key",
        "change",
        "Set key",
        &["key", "scale", "major", "minor", "mode"],
    ),
    m(
        "remove_key",
        "remove",
        "Remove key",
        &["key", "scale", "delete", "not sure"],
    ),
    m(
        "set_marker",
        "add",
        "Add marker",
        &["marker", "label", "section", "name", "bookmark"],
    ),
    m(
        "remove_marker",
        "remove",
        "Remove marker",
        &["marker", "label", "section", "delete"],
    ),
    m(
        "set_region_loop",
        "change",
        "Loop clip",
        &["loop", "repeat", "cycle", "fill", "clip"],
    ),
    m(
        "add_track",
        "add",
        "Add track",
        &["instrument", "part", "layer", "new"],
    ),
    m(
        "rename_track",
        "change",
        "Rename track",
        &["name", "title", "label"],
    ),
    m(
        "set_instrument",
        "change",
        "Change sound",
        &["instrument", "voice", "patch", "timbre"],
    ),
    m(
        "move_track",
        "change",
        "Reorder track",
        &["order", "position", "swap"],
    ),
    m(
        "delete_track",
        "remove",
        "Remove track",
        &["delete", "erase", "part"],
    ),
    m(
        "add_region",
        "add",
        "Add clip",
        &["region", "block", "section", "new"],
    ),
    m(
        "add_audio_clip",
        "add",
        "Add audio clip",
        &["audio", "sample", "recording", "wav", "import", "clip"],
    ),
    m(
        "set_clip_gain",
        "change",
        "Change audio clip volume",
        &["audio", "gain", "volume", "louder", "quieter", "clip"],
    ),
    m(
        "set_clip_offset",
        "change",
        "Trim audio clip start",
        &["audio", "trim", "offset", "start", "clip"],
    ),
    m(
        "move_region",
        "change",
        "Move clip",
        &["shift", "slide", "region", "time"],
    ),
    m(
        "resize_region",
        "change",
        "Resize clip",
        &["length", "trim", "stretch", "region"],
    ),
    m(
        "delete_region",
        "remove",
        "Remove clip",
        &["delete", "erase", "region"],
    ),
    m("add_note", "add", "Add note", &["pitch", "key", "new"]),
    m(
        "move_note",
        "change",
        "Move note",
        &["pitch", "shift", "time"],
    ),
    m(
        "resize_note",
        "change",
        "Change note length",
        &["duration", "longer", "shorter"],
    ),
    m(
        "set_note_velocity",
        "change",
        "Change note loudness",
        &["velocity", "volume", "dynamics"],
    ),
    m("delete_note", "remove", "Remove note", &["delete", "erase"]),
    m(
        "move_region_to_track",
        "change",
        "Move clip to another track",
        &["region", "send", "transfer"],
    ),
    m(
        "set_track_volume",
        "change",
        "Change track volume",
        &["level", "fader", "louder", "quieter"],
    ),
    m(
        "set_master_volume",
        "change",
        "Change master volume",
        &["level", "loudness", "louder", "quieter", "output"],
    ),
    m(
        "set_master_reverb",
        "change",
        "Change master reverb",
        &["room", "space", "echo", "hall", "wet"],
    ),
    m(
        "set_track_reverb_send",
        "change",
        "Change track reverb amount",
        &["room", "space", "echo", "wet", "send"],
    ),
    m(
        "add_automation_point",
        "add",
        "Add automation point",
        &["automate", "fade", "curve", "envelope", "ramp"],
    ),
    m(
        "move_automation_point",
        "change",
        "Move automation point",
        &["automate", "fade", "curve", "envelope"],
    ),
    m(
        "set_automation_curve",
        "change",
        "Change automation curve",
        &["automate", "fade", "curve", "smooth", "step"],
    ),
    m(
        "delete_automation_point",
        "remove",
        "Remove automation point",
        &["automate", "fade", "curve", "delete"],
    ),
    m(
        "clear_automation_lane",
        "remove",
        "Clear automation",
        &["automate", "fade", "curve", "reset", "delete"],
    ),
    m(
        "set_track_pan",
        "change",
        "Change track pan",
        &["left", "right", "stereo", "balance"],
    ),
    m(
        "set_track_mute",
        "change",
        "Mute or unmute track",
        &["silence", "quiet", "off"],
    ),
    m(
        "set_track_solo",
        "change",
        "Solo or unsolo track",
        &["alone", "isolate", "only"],
    ),
    m(
        "set_track_sound",
        "change",
        "Change track sound",
        &["voice", "instrument", "timbre", "preset"],
    ),
    m(
        "add_effect",
        "add",
        "Add effect",
        &[
            "effect",
            "delay",
            "gate",
            "crusher",
            "distortion",
            "tremolo",
            "echo",
            "plugin",
        ],
    ),
    m(
        "remove_effect",
        "remove",
        "Remove effect",
        &["effect", "delete", "device"],
    ),
    m(
        "move_effect",
        "change",
        "Reorder effects",
        &["effect", "order", "chain", "device"],
    ),
    m(
        "set_effect_bypass",
        "change",
        "Bypass effect",
        &["effect", "bypass", "off", "on", "device"],
    ),
    m(
        "set_effect_param",
        "change",
        "Change effect setting",
        &["effect", "knob", "amount", "time", "feedback", "drive"],
    ),
    m(
        "add_notes",
        "add",
        "Add several notes",
        &["chord", "paste", "many", "melody"],
    ),
    m(
        "split_region",
        "change",
        "Split clip",
        &["cut", "divide", "razor", "region"],
    ),
    m(
        "duplicate_region",
        "add",
        "Duplicate clip",
        &["copy", "repeat", "clone", "region"],
    ),
    m(
        "transpose_notes",
        "change",
        "Transpose notes",
        &["pitch", "semitone", "up", "down", "key"],
    ),
    m(
        "quantize_notes",
        "change",
        "Tidy note timing",
        &["quantize", "grid", "snap", "straighten"],
    ),
    m(
        "move_notes",
        "change",
        "Move several notes",
        &["shift", "drag", "nudge", "selection"],
    ),
    m(
        "resize_notes",
        "change",
        "Resize several notes",
        &["length", "longer", "shorter", "stretch"],
    ),
    m(
        "set_notes_velocity",
        "change",
        "Change note loudness",
        &["velocity", "dynamics", "ramp", "softer", "louder"],
    ),
    m(
        "delete_notes",
        "remove",
        "Delete several notes",
        &["erase", "clear", "selection", "cut"],
    ),
    m(
        "conform_notes_to_scale",
        "change",
        "Fit notes to the key",
        &["scale", "key", "conform", "snap", "wrong notes", "in tune"],
    ),
];

fn meta_of(name: &str) -> Option<&'static Meta> {
    META.iter().find(|x| x.name == name)
}

/// What a palette needs to list one command (D33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CommandMeta {
    pub verb: &'static str,
    pub label: &'static str,
    pub keywords: &'static [&'static str],
}

/// Palette metadata of a described command.
pub fn command_meta(name: &str) -> Option<CommandMeta> {
    meta_of(name).map(|x| CommandMeta {
        verb: x.verb,
        label: x.label,
        keywords: x.keywords,
    })
}

/// Optional narrowing for `commands_enabled`.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnabledContext {
    pub track: Option<Id>,
    pub region: Option<Id>,
}

fn midi_regions(p: &Project) -> impl Iterator<Item = &composer_model::Region> {
    p.tracks
        .iter()
        .flat_map(|t| t.regions.values())
        .filter(|r| matches!(r.content, RegionContent::Midi { .. }))
}

/// Why `name` cannot run now, or None when it can. Looks at the project only.
fn reason(p: &Project, ctx: EnabledContext, name: &str) -> Option<&'static str> {
    let has_track = !p.tracks.is_empty();
    let any_region = p.tracks.iter().any(|t| !t.regions.is_empty());
    let any_note = midi_regions(p)
        .any(|r| matches!(&r.content, RegionContent::Midi { notes } if !notes.is_empty()));
    if let Some(t) = ctx.track {
        if p.track(t).is_none() {
            return Some("That track no longer exists.");
        }
    }
    if let Some(r) = ctx.region {
        if p.region(r).is_none() {
            return Some("That clip no longer exists.");
        }
    }
    match name {
        "remove_tempo" if p.tempo_map.tempos.len() < 2 => Some("There is only one tempo."),
        "remove_key" if p.tempo_map.keys.is_empty() => Some("No key is set."),
        "move_automation_point"
        | "set_automation_curve"
        | "delete_automation_point"
        | "clear_automation_lane"
            if p.automation.is_empty() =>
        {
            Some("There is no automation yet.")
        }
        "add_automation_point" if !has_track => Some("Add a track first."),
        "remove_marker" if p.markers.is_empty() => Some("There are no markers."),
        "add_audio_clip" if !p.tracks.iter().any(|t| t.kind == TrackKind::Audio) => {
            Some("Add an audio track first.")
        }
        "set_clip_gain" | "set_clip_offset"
            if !p
                .tracks
                .iter()
                .flat_map(|t| t.regions.values())
                .any(|r| matches!(r.content, RegionContent::Clip { .. })) =>
        {
            Some("Import an audio file first.")
        }
        "remove_effect" | "move_effect" | "set_effect_bypass" | "set_effect_param"
            if p.effects.is_empty() =>
        {
            Some("Add an effect first.")
        }
        "move_effect" if p.effects.iter().all(|c| c.effects.len() < 2) => {
            Some("You need two effects in one chain to reorder them.")
        }
        "set_region_loop" if midi_regions(p).next().is_none() => Some("Add a music clip first."),
        "remove_time_signature" if p.tempo_map.meters.len() < 2 => {
            Some("There is only one time signature.")
        }
        "rename_track"
        | "move_track"
        | "delete_track"
        | "set_track_volume"
        | "set_track_pan"
        | "set_track_mute"
        | "set_track_solo"
        | "set_track_sound"
        | "add_region"
        | "set_track_reverb_send"
            if !has_track =>
        {
            Some("Add a track first.")
        }
        "move_track" if p.tracks.len() < 2 => Some("You need two tracks to reorder them."),
        "set_instrument" if !p.tracks.iter().any(|t| t.kind == TrackKind::Instrument) => {
            Some("Add an instrument track first.")
        }
        "move_region_to_track" if p.tracks.len() < 2 => Some("You need a second track."),
        "move_region"
        | "resize_region"
        | "delete_region"
        | "split_region"
        | "duplicate_region"
        | "move_region_to_track"
            if !any_region =>
        {
            Some("Add a clip first.")
        }
        "add_note" | "add_notes" if midi_regions(p).next().is_none() => {
            Some("Add a music clip first.")
        }
        "conform_notes_to_scale" if p.tempo_map.keys.is_empty() => Some("Set a key first."),
        "move_note"
        | "resize_note"
        | "set_note_velocity"
        | "delete_note"
        | "transpose_notes"
        | "quantize_notes"
        | "move_notes"
        | "resize_notes"
        | "set_notes_velocity"
        | "delete_notes"
        | "conform_notes_to_scale"
            if !any_note =>
        {
            Some("Add a note first.")
        }
        _ => None,
    }
}

/// Availability of one command now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EnabledEntry {
    pub name: &'static str,
    pub enabled: bool,
    /// Plain sentence when disabled.
    pub reason: Option<&'static str>,
}

/// Per command availability with a plain reason when disabled.
pub fn commands_enabled(p: &Project, ctx: EnabledContext) -> Vec<EnabledEntry> {
    describe()
        .iter()
        .map(|c| {
            let r = reason(p, ctx, c.name);
            EnabledEntry {
                name: c.name,
                enabled: r.is_none(),
                reason: r,
            }
        })
        .collect()
}
