use crate::error::CommandError;
use composer_model::{
    AutomationCurve, AutomationPoint, AutomationTarget, Change, ChannelMix, Effect, EffectKind,
    EffectOwner, FieldKey, FieldValue, Id, Key, MasterMix, Note, Project, Region, RegionContent,
    RegionKind, Scale, Tick, TimeSignature, Track, TrackKind, MAX_AUTOMATION_POINTS,
    MAX_CHAIN_EFFECTS, MAX_CLIP_OFFSET, MAX_MARKERS, MAX_MBPM, MAX_NAME_LEN, MAX_PAN, MAX_REVERB,
    MAX_SOUND_ID_LEN, MAX_TICK, MAX_VOLUME, MIN_MBPM, TICKS_PER_QUARTER,
};
use composer_theory::nearest_in_key;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Most notes one bulk or clip command may carry or touch (D15).
pub const MAX_BULK_NOTES: usize = 100_000;

/// One note of an `add_notes` command; `start` is relative to the clip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteSpec {
    pub id: Id,
    pub pitch: u8,
    pub velocity: u8,
    pub start: Tick,
    pub duration: Tick,
}

/// Linear velocity ramp over the picked notes of `set_notes_velocity` (D22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VelocityRamp {
    pub from: u8,
    pub to: u8,
}

/// One semantic edit. Serialized as JSON with a `command` tag holding the
/// stable name (see [`Command::name`]) next to the fields. Ids for new things
/// are supplied by the caller, so replaying a command is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    SetProjectName {
        name: String,
    },
    SetTempo {
        tick: Tick,
        mbpm: u32,
    },
    RemoveTempo {
        tick: Tick,
    },
    SetTimeSignature {
        tick: Tick,
        numerator: u8,
        denominator: u8,
    },
    RemoveTimeSignature {
        tick: Tick,
    },
    SetKey {
        tick: Tick,
        root: u8,
        scale: Scale,
    },
    RemoveKey {
        tick: Tick,
    },
    /// Adds a marker at a tick, or renames the one already there (D24).
    SetMarker {
        tick: Tick,
        name: String,
    },
    RemoveMarker {
        tick: Tick,
    },
    /// Sets how many ticks of a midi clip repeat to fill it; 0 clears the loop (D24).
    SetRegionLoop {
        region: Id,
        loop_length: Tick,
    },
    AddTrack {
        id: Id,
        name: String,
        kind: TrackKind,
        index: Option<usize>,
    },
    RenameTrack {
        track: Id,
        name: String,
    },
    SetInstrument {
        track: Id,
        instrument: Option<Id>,
    },
    /// Catalogue sound id of an instrument track, or null to clear it (D29).
    SetTrackSound {
        track: Id,
        sound: Option<String>,
    },
    MoveTrack {
        track: Id,
        index: usize,
    },
    DeleteTrack {
        track: Id,
    },
    AddRegion {
        id: Id,
        track: Id,
        start: Tick,
        length: Tick,
        kind: RegionKind,
    },
    MoveRegion {
        region: Id,
        start: Tick,
    },
    ResizeRegion {
        region: Id,
        length: Tick,
    },
    DeleteRegion {
        region: Id,
    },
    AddNote {
        id: Id,
        region: Id,
        pitch: u8,
        velocity: u8,
        start: Tick,
        duration: Tick,
    },
    MoveNote {
        region: Id,
        note: Id,
        start: Tick,
        pitch: u8,
    },
    ResizeNote {
        region: Id,
        note: Id,
        duration: Tick,
    },
    SetNoteVelocity {
        region: Id,
        note: Id,
        velocity: u8,
    },
    DeleteNote {
        region: Id,
        note: Id,
    },
    /// Adds an audio clip that plays the asset with this content hash (D40). `offset` is the
    /// first source frame (null is 0), `gain` is permille of unity (null is 1000).
    AddAudioClip {
        id: Id,
        track: Id,
        start: Tick,
        length: Tick,
        asset: String,
        offset: Option<u64>,
        gain: Option<u16>,
    },
    /// Sets the gain of an audio clip in permille of unity, 0..=2000 (D40).
    SetClipGain {
        region: Id,
        gain: u16,
    },
    /// Sets the first source frame an audio clip plays (D40).
    SetClipOffset {
        region: Id,
        offset: u64,
    },
    /// Moves a clip to another track of the same kind; start tick and notes are kept.
    MoveRegionToTrack {
        region: Id,
        track: Id,
    },
    SetTrackVolume {
        track: Id,
        /// Permille of unity gain, 0..=2000.
        volume: u16,
    },
    SetTrackPan {
        track: Id,
        /// Permille, -1000 full left to 1000 full right.
        pan: i16,
    },
    SetTrackMute {
        track: Id,
        mute: bool,
    },
    SetTrackSolo {
        track: Id,
        solo: bool,
    },
    /// Master volume in permille of unity gain, 0..=2000 (D26).
    SetMasterVolume {
        volume: u16,
    },
    /// Master reverb room size and return level, each 0..=1000 permille (D26).
    SetMasterReverb {
        size: u16,
        mix: u16,
    },
    /// How much of a track feeds the master reverb, 0..=1000 permille (D26).
    SetTrackReverbSend {
        track: Id,
        send: u16,
    },
    /// Adds an automation point to a lane, creating the lane when it is the first (D36).
    /// `value` is in the unit of the target (permille). A point already at the tick is an error.
    AddAutomationPoint {
        target: AutomationTarget,
        tick: Tick,
        value: i16,
        /// How the value travels to the next point; linear when absent.
        #[serde(default)]
        curve: Option<AutomationCurve>,
    },
    /// Moves a point to a new tick and value, keeping its curve (D36).
    MoveAutomationPoint {
        target: AutomationTarget,
        tick: Tick,
        new_tick: Tick,
        value: i16,
    },
    /// Changes how the value travels from a point to the next one (D36).
    SetAutomationCurve {
        target: AutomationTarget,
        tick: Tick,
        curve: AutomationCurve,
    },
    DeleteAutomationPoint {
        target: AutomationTarget,
        tick: Tick,
    },
    /// Removes every point of a lane, and with them the lane (D36).
    ClearAutomationLane {
        target: AutomationTarget,
    },
    /// Adds many notes to one midi clip as one command.
    AddNotes {
        region: Id,
        notes: Vec<NoteSpec>,
    },
    /// Cuts a clip at an absolute tick. The right part gets `new_region`; a note
    /// crossing the cut is shortened on the left and continues on the right under
    /// the next id of `note_ids` (one id per crossing note, in note id order).
    SplitRegion {
        region: Id,
        at: Tick,
        new_region: Id,
        note_ids: Vec<Id>,
    },
    /// Copies a clip to `start` on `track` (null keeps the track). `note_ids`
    /// holds one new id per copied note, in note id order.
    DuplicateRegion {
        region: Id,
        new_region: Id,
        start: Tick,
        track: Option<Id>,
        note_ids: Vec<Id>,
    },
    /// Shifts pitches of the listed notes (null means every note of the clip).
    TransposeNotes {
        region: Id,
        semitones: i16,
        notes: Option<Vec<Id>>,
    },
    /// Pulls note starts toward the grid by `strength` percent (1..=100).
    QuantizeNotes {
        region: Id,
        grid: Tick,
        strength: u8,
        notes: Option<Vec<Id>>,
    },
    /// Shifts the listed notes by ticks and semitones as one edit (D22).
    MoveNotes {
        region: Id,
        notes: Vec<Id>,
        delta_tick: Tick,
        delta_pitch: i16,
    },
    /// Adds `delta` ticks to the duration of each listed note (D22).
    ResizeNotes {
        region: Id,
        notes: Vec<Id>,
        delta: Tick,
    },
    /// Sets velocity of the listed notes to `value`, or along `ramp` by note start.
    /// Exactly one of the two must be given (D22).
    SetNotesVelocity {
        region: Id,
        notes: Vec<Id>,
        value: Option<u8>,
        ramp: Option<VelocityRamp>,
    },
    /// Removes the listed notes (D22).
    DeleteNotes {
        region: Id,
        notes: Vec<Id>,
    },
    /// Moves notes outside the key in effect at their start to the nearest in key pitch,
    /// the lower one on a tie (null notes means every note of the clip) (D23b).
    ConformNotesToScale {
        region: Id,
        notes: Option<Vec<Id>>,
    },
    /// Adds a device with default parameters to the chain of a track or the master, at `index`
    /// (null appends) (D45).
    AddEffect {
        id: Id,
        owner: EffectOwner,
        kind: EffectKind,
        index: Option<usize>,
    },
    RemoveEffect {
        effect: Id,
    },
    /// Moves a device to a new position of its chain (D45).
    MoveEffect {
        effect: Id,
        index: usize,
    },
    SetEffectBypass {
        effect: Id,
        bypass: bool,
    },
    /// Sets one parameter of a device by its name (D45).
    SetEffectParam {
        effect: Id,
        param: String,
        value: i32,
    },
}

fn invalid(field: &'static str, reason: &'static str) -> CommandError {
    CommandError::Invalid { field, reason }
}

fn check_name(name: &str) -> Result<(), CommandError> {
    if name.len() > MAX_NAME_LEN {
        return Err(invalid("name", "too long"));
    }
    Ok(())
}

fn check_tick(field: &'static str, t: Tick) -> Result<(), CommandError> {
    if !(0..=MAX_TICK).contains(&t) {
        return Err(invalid(field, "out of range"));
    }
    Ok(())
}

fn check_span(field: &'static str, t: Tick) -> Result<(), CommandError> {
    if !(1..=MAX_TICK).contains(&t) {
        return Err(invalid(field, "must be positive and within range"));
    }
    Ok(())
}

fn check_pitch(p: u8) -> Result<(), CommandError> {
    if p > 127 {
        return Err(invalid("pitch", "must be 0 to 127"));
    }
    Ok(())
}

fn check_velocity(v: u8) -> Result<(), CommandError> {
    if !(1..=127).contains(&v) {
        return Err(invalid("velocity", "must be 1 to 127"));
    }
    Ok(())
}

/// A note must lie inside its region: start >= 0 and start + duration <= region length.
fn check_inside(start: Tick, duration: Tick, length: Tick) -> Result<(), CommandError> {
    if start + duration > length {
        return Err(CommandError::OutOfBounds {
            field: "note",
            reason: "must lie inside its clip",
        });
    }
    Ok(())
}

fn fresh(p: &Project, id: Id) -> Result<(), CommandError> {
    if p.contains_id(id) {
        return Err(CommandError::DuplicateId(id));
    }
    Ok(())
}

fn track(p: &Project, id: Id) -> Result<&Track, CommandError> {
    p.track(id)
        .ok_or(CommandError::NotFound { what: "track", id })
}

fn effect(p: &Project, id: Id) -> Result<(EffectOwner, usize, &Effect), CommandError> {
    p.effect(id)
        .ok_or(CommandError::NotFound { what: "effect", id })
}

fn region(p: &Project, id: Id) -> Result<(&Track, &Region), CommandError> {
    p.region(id)
        .ok_or(CommandError::NotFound { what: "region", id })
}

fn note(p: &Project, region_id: Id, id: Id) -> Result<&Note, CommandError> {
    region(p, region_id)?;
    p.note(region_id, id)
        .ok_or(CommandError::NotFound { what: "note", id })
}

/// The target must exist (its track, when it has one).
fn check_target(p: &Project, target: &AutomationTarget) -> Result<(), CommandError> {
    match target.track() {
        Some(t) => track(p, t).map(|_| ()),
        None => Ok(()),
    }
}

fn check_value(target: &AutomationTarget, value: i16) -> Result<(), CommandError> {
    if target.accepts(value) {
        Ok(())
    } else {
        Err(invalid("value", "outside the range of this target"))
    }
}

fn point_at(
    p: &Project,
    target: &AutomationTarget,
    tick: Tick,
) -> Result<AutomationPoint, CommandError> {
    p.automation_point(target, tick)
        .ok_or_else(|| invalid("tick", "no automation point at this tick"))
}

/// One removal per point, last point first, so undo re-adds them in order.
fn clear_lane(target: &AutomationTarget, lane: &composer_model::AutomationLane) -> Vec<Change> {
    lane.points
        .iter()
        .rev()
        .map(|(tick, pt)| Change::SetAutomationPoint {
            target: *target,
            tick: *tick,
            old: Some(*pt),
            new: None,
        })
        .collect()
}

/// Builds a `Set` change, or nothing when the value does not change.
fn set(key: FieldKey, old: FieldValue, new: FieldValue) -> Vec<Change> {
    if old == new {
        Vec::new()
    } else {
        vec![Change::Set { key, old, new }]
    }
}

fn mix_change(
    p: &Project,
    id: Id,
    edit: impl FnOnce(&mut ChannelMix),
) -> Result<Vec<Change>, CommandError> {
    let old = track(p, id)?.mix;
    let mut new = old;
    edit(&mut new);
    Ok(set(
        FieldKey::TrackMix { track: id },
        FieldValue::Mix(old),
        FieldValue::Mix(new),
    ))
}

/// The midi notes of a clip, or an error when the clip is audio.
fn midi_notes(r: &Region) -> Result<&BTreeMap<Id, Note>, CommandError> {
    match &r.content {
        RegionContent::Midi { notes } => Ok(notes),
        RegionContent::Audio | RegionContent::Clip { .. } => Err(CommandError::WrongKind {
            reason: "notes live in midi clips",
        }),
    }
}

/// Notes named by `ids` (every note when None). Unknown or repeated ids are rejected.
fn pick_notes<'a>(
    notes: &'a BTreeMap<Id, Note>,
    ids: &Option<Vec<Id>>,
) -> Result<Vec<&'a Note>, CommandError> {
    match ids {
        None => Ok(notes.values().collect()),
        Some(list) => pick_listed(notes, list),
    }
}

/// Notes named by a non empty `list`; unknown, repeated or too many ids are rejected.
fn pick_listed<'a>(
    notes: &'a BTreeMap<Id, Note>,
    list: &[Id],
) -> Result<Vec<&'a Note>, CommandError> {
    if list.is_empty() {
        return Err(invalid("notes", "at least one note id"));
    }
    if list.len() > MAX_BULK_NOTES {
        return Err(invalid("notes", "too many ids"));
    }
    let mut seen = HashSet::new();
    list.iter()
        .map(|id| {
            if !seen.insert(*id) {
                return Err(invalid("notes", "repeated id"));
            }
            notes.get(id).ok_or(CommandError::NotFound {
                what: "note",
                id: *id,
            })
        })
        .collect()
}

/// New ids must be unused in the project and unique among themselves.
fn fresh_all(p: &Project, ids: impl IntoIterator<Item = Id>) -> Result<(), CommandError> {
    let mut seen = HashSet::new();
    for id in ids {
        fresh(p, id)?;
        if !seen.insert(id) {
            return Err(CommandError::DuplicateId(id));
        }
    }
    Ok(())
}

impl Command {
    /// Stable name clients send in the `command` field.
    pub fn name(&self) -> &'static str {
        match self {
            Command::SetProjectName { .. } => "set_project_name",
            Command::SetTempo { .. } => "set_tempo",
            Command::RemoveTempo { .. } => "remove_tempo",
            Command::SetTimeSignature { .. } => "set_time_signature",
            Command::RemoveTimeSignature { .. } => "remove_time_signature",
            Command::SetKey { .. } => "set_key",
            Command::RemoveKey { .. } => "remove_key",
            Command::SetMarker { .. } => "set_marker",
            Command::RemoveMarker { .. } => "remove_marker",
            Command::SetRegionLoop { .. } => "set_region_loop",
            Command::AddTrack { .. } => "add_track",
            Command::RenameTrack { .. } => "rename_track",
            Command::SetInstrument { .. } => "set_instrument",
            Command::SetTrackSound { .. } => "set_track_sound",
            Command::MoveTrack { .. } => "move_track",
            Command::DeleteTrack { .. } => "delete_track",
            Command::AddRegion { .. } => "add_region",
            Command::MoveRegion { .. } => "move_region",
            Command::ResizeRegion { .. } => "resize_region",
            Command::DeleteRegion { .. } => "delete_region",
            Command::AddNote { .. } => "add_note",
            Command::MoveNote { .. } => "move_note",
            Command::ResizeNote { .. } => "resize_note",
            Command::SetNoteVelocity { .. } => "set_note_velocity",
            Command::DeleteNote { .. } => "delete_note",
            Command::MoveRegionToTrack { .. } => "move_region_to_track",
            Command::SetTrackVolume { .. } => "set_track_volume",
            Command::SetTrackPan { .. } => "set_track_pan",
            Command::SetTrackMute { .. } => "set_track_mute",
            Command::SetMasterVolume { .. } => "set_master_volume",
            Command::SetMasterReverb { .. } => "set_master_reverb",
            Command::SetTrackReverbSend { .. } => "set_track_reverb_send",
            Command::SetTrackSolo { .. } => "set_track_solo",
            Command::AddAudioClip { .. } => "add_audio_clip",
            Command::SetClipGain { .. } => "set_clip_gain",
            Command::SetClipOffset { .. } => "set_clip_offset",
            Command::AddAutomationPoint { .. } => "add_automation_point",
            Command::MoveAutomationPoint { .. } => "move_automation_point",
            Command::SetAutomationCurve { .. } => "set_automation_curve",
            Command::DeleteAutomationPoint { .. } => "delete_automation_point",
            Command::ClearAutomationLane { .. } => "clear_automation_lane",
            Command::AddNotes { .. } => "add_notes",
            Command::SplitRegion { .. } => "split_region",
            Command::DuplicateRegion { .. } => "duplicate_region",
            Command::TransposeNotes { .. } => "transpose_notes",
            Command::QuantizeNotes { .. } => "quantize_notes",
            Command::MoveNotes { .. } => "move_notes",
            Command::ResizeNotes { .. } => "resize_notes",
            Command::SetNotesVelocity { .. } => "set_notes_velocity",
            Command::DeleteNotes { .. } => "delete_notes",
            Command::ConformNotesToScale { .. } => "conform_notes_to_scale",
            Command::AddEffect { .. } => "add_effect",
            Command::RemoveEffect { .. } => "remove_effect",
            Command::MoveEffect { .. } => "move_effect",
            Command::SetEffectBypass { .. } => "set_effect_bypass",
            Command::SetEffectParam { .. } => "set_effect_param",
        }
    }

    /// Validates against `p` and returns the low level changes, without touching `p`.
    /// An empty list means the command is a no-op.
    pub fn plan(&self, p: &Project) -> Result<Vec<Change>, CommandError> {
        match self {
            Command::SetProjectName { name } => {
                check_name(name)?;
                Ok(set(
                    FieldKey::ProjectName,
                    FieldValue::Text(p.name.clone()),
                    FieldValue::Text(name.clone()),
                ))
            }
            Command::SetTempo { tick, mbpm } => {
                check_tick("tick", *tick)?;
                if !(MIN_MBPM..=MAX_MBPM).contains(mbpm) {
                    return Err(invalid("mbpm", "outside the allowed tempo range"));
                }
                let old = p.tempo_map.tempos.get(tick).copied();
                if old == Some(*mbpm) {
                    return Ok(Vec::new());
                }
                Ok(vec![Change::SetTempo {
                    tick: *tick,
                    old,
                    new: Some(*mbpm),
                }])
            }
            Command::RemoveTempo { tick } => {
                if *tick == 0 {
                    return Err(invalid("tick", "the tempo at tick 0 cannot be removed"));
                }
                let old = p.tempo_map.tempos.get(tick).copied();
                if old.is_none() {
                    return Err(invalid("tick", "no tempo change at this tick"));
                }
                Ok(vec![Change::SetTempo {
                    tick: *tick,
                    old,
                    new: None,
                }])
            }
            Command::SetTimeSignature {
                tick,
                numerator,
                denominator,
            } => {
                check_tick("tick", *tick)?;
                let sig = TimeSignature {
                    numerator: *numerator,
                    denominator: *denominator,
                };
                if !sig.is_valid() {
                    return Err(invalid("time_signature", "unsupported signature"));
                }
                let old = p.tempo_map.meters.get(tick).copied();
                if old == Some(sig) {
                    return Ok(Vec::new());
                }
                Ok(vec![Change::SetMeter {
                    tick: *tick,
                    old,
                    new: Some(sig),
                }])
            }
            Command::RemoveTimeSignature { tick } => {
                if *tick == 0 {
                    return Err(invalid("tick", "the signature at tick 0 cannot be removed"));
                }
                let old = p.tempo_map.meters.get(tick).copied();
                if old.is_none() {
                    return Err(invalid("tick", "no signature change at this tick"));
                }
                Ok(vec![Change::SetMeter {
                    tick: *tick,
                    old,
                    new: None,
                }])
            }
            Command::SetKey { tick, root, scale } => {
                check_tick("tick", *tick)?;
                if *root > 11 {
                    return Err(invalid("root", "must be 0 (C) to 11 (B)"));
                }
                let key = Key {
                    root: *root,
                    scale: *scale,
                };
                let old = p.tempo_map.keys.get(tick).copied();
                if old == Some(key) {
                    return Ok(Vec::new());
                }
                Ok(vec![Change::SetKey {
                    tick: *tick,
                    old,
                    new: Some(key),
                }])
            }
            Command::RemoveKey { tick } => {
                let old = p.tempo_map.keys.get(tick).copied();
                if old.is_none() {
                    return Err(invalid("tick", "no key change at this tick"));
                }
                Ok(vec![Change::SetKey {
                    tick: *tick,
                    old,
                    new: None,
                }])
            }
            Command::SetMarker { tick, name } => {
                check_tick("tick", *tick)?;
                check_name(name)?;
                let old = p.markers.get(tick).cloned();
                if old.as_deref() == Some(name.as_str()) {
                    return Ok(Vec::new());
                }
                if old.is_none() && p.markers.len() >= MAX_MARKERS {
                    return Err(invalid("tick", "too many markers"));
                }
                Ok(vec![Change::SetMarker {
                    tick: *tick,
                    old,
                    new: Some(name.clone()),
                }])
            }
            Command::RemoveMarker { tick } => {
                let old = p.markers.get(tick).cloned();
                if old.is_none() {
                    return Err(invalid("tick", "no marker at this tick"));
                }
                Ok(vec![Change::SetMarker {
                    tick: *tick,
                    old,
                    new: None,
                }])
            }
            Command::SetRegionLoop {
                region: id,
                loop_length,
            } => {
                let (_, r) = region(p, *id)?;
                midi_notes(r)?;
                check_tick("loop_length", *loop_length)?;
                if *loop_length != 0 && *loop_length < TICKS_PER_QUARTER / 4 {
                    return Err(invalid(
                        "loop_length",
                        "must be 0 or at least a sixteenth note",
                    ));
                }
                if r.loop_length == *loop_length {
                    return Ok(Vec::new());
                }
                Ok(set(
                    FieldKey::RegionLoop { region: *id },
                    FieldValue::Tick(r.loop_length),
                    FieldValue::Tick(*loop_length),
                ))
            }
            Command::AddTrack {
                id,
                name,
                kind,
                index,
            } => {
                check_name(name)?;
                fresh(p, *id)?;
                let index = index.unwrap_or(p.tracks.len());
                if index > p.tracks.len() {
                    return Err(invalid("index", "past the end of the track list"));
                }
                let t = Track {
                    id: *id,
                    name: name.clone(),
                    kind: *kind,
                    instrument: None,
                    mix: Default::default(),
                    sound: None,
                    reverb_send: 0,
                    regions: BTreeMap::new(),
                };
                Ok(vec![Change::NewTrack { index, track: t }])
            }
            Command::RenameTrack { track: id, name } => {
                check_name(name)?;
                let t = track(p, *id)?;
                Ok(set(
                    FieldKey::TrackName { track: *id },
                    FieldValue::Text(t.name.clone()),
                    FieldValue::Text(name.clone()),
                ))
            }
            Command::SetInstrument {
                track: id,
                instrument,
            } => {
                let t = track(p, *id)?;
                if t.kind != TrackKind::Instrument {
                    return Err(CommandError::WrongKind {
                        reason: "only instrument tracks have an instrument slot",
                    });
                }
                Ok(set(
                    FieldKey::TrackInstrument { track: *id },
                    FieldValue::Slot(t.instrument),
                    FieldValue::Slot(*instrument),
                ))
            }
            Command::SetTrackSound { track: id, sound } => {
                let t = track(p, *id)?;
                if t.kind != TrackKind::Instrument {
                    return Err(CommandError::WrongKind {
                        reason: "only instrument tracks have a sound",
                    });
                }
                if let Some(s) = sound {
                    let ok = !s.is_empty()
                        && s.len() <= MAX_SOUND_ID_LEN
                        && s.bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
                    if !ok {
                        return Err(invalid(
                            "sound",
                            "1 to 64 characters of a to z, 0 to 9 and hyphen",
                        ));
                    }
                }
                Ok(set(
                    FieldKey::TrackSound { track: *id },
                    FieldValue::Sound(t.sound.clone()),
                    FieldValue::Sound(sound.clone()),
                ))
            }
            Command::MoveTrack { track: id, index } => {
                let from = p.track_index(*id).ok_or(CommandError::NotFound {
                    what: "track",
                    id: *id,
                })?;
                if *index >= p.tracks.len() {
                    return Err(invalid("index", "out of range"));
                }
                if from == *index {
                    return Ok(Vec::new());
                }
                let t = p.tracks[from].clone();
                Ok(vec![
                    Change::DeleteTrack {
                        index: from,
                        track: t.clone(),
                    },
                    Change::NewTrack {
                        index: *index,
                        track: t,
                    },
                ])
            }
            Command::DeleteTrack { track: id } => {
                let from = p.track_index(*id).ok_or(CommandError::NotFound {
                    what: "track",
                    id: *id,
                })?;
                // Lanes go first, so undo restores the track before its lanes (D36).
                let mut out = Vec::new();
                for lane in p
                    .automation
                    .iter()
                    .filter(|l| l.target.track() == Some(*id))
                {
                    out.extend(clear_lane(&lane.target, lane));
                }
                // Devices go too, last first, so undo restores them in order (D45).
                let owner = EffectOwner::Track { track: *id };
                for (i, e) in p.chain(&owner).iter().enumerate().rev() {
                    out.push(Change::RemoveEffect {
                        owner,
                        index: i,
                        effect: e.clone(),
                    });
                }
                out.push(Change::DeleteTrack {
                    index: from,
                    track: p.tracks[from].clone(),
                });
                Ok(out)
            }
            Command::AddRegion {
                id,
                track: track_id,
                start,
                length,
                kind,
            } => {
                fresh(p, *id)?;
                let t = track(p, *track_id)?;
                // Clips on one track may overlap on purpose: layered takes and comping are
                // normal, and overlap is a playback concern, not a document error.
                check_tick("start", *start)?;
                check_span("length", *length)?;
                let content =
                    match (kind, t.kind) {
                        (RegionKind::Midi, TrackKind::Instrument) => RegionContent::Midi {
                            notes: BTreeMap::new(),
                        },
                        (RegionKind::Audio, TrackKind::Audio) => RegionContent::Audio,
                        _ => return Err(CommandError::WrongKind {
                            reason:
                                "midi clips go on instrument tracks, audio clips on audio tracks",
                        }),
                    };
                let r = Region {
                    id: *id,
                    start: *start,
                    length: *length,
                    loop_length: 0,
                    content,
                };
                Ok(vec![Change::NewRegion {
                    track: *track_id,
                    region: r,
                }])
            }
            Command::AddAudioClip {
                id,
                track: track_id,
                start,
                length,
                asset,
                offset,
                gain,
            } => {
                fresh(p, *id)?;
                let t = track(p, *track_id)?;
                if t.kind != TrackKind::Audio {
                    return Err(CommandError::WrongKind {
                        reason: "audio clips go on audio tracks",
                    });
                }
                check_tick("start", *start)?;
                check_span("length", *length)?;
                if asset.len() != 64
                    || !asset
                        .bytes()
                        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                {
                    return Err(invalid("asset", "must be 64 lowercase hex digits"));
                }
                let offset = offset.unwrap_or(0);
                if offset > MAX_CLIP_OFFSET {
                    return Err(invalid("offset", "out of range"));
                }
                let gain = gain.unwrap_or(1000);
                if gain > MAX_VOLUME {
                    return Err(invalid("gain", "must be 0 to 2000"));
                }
                Ok(vec![Change::NewRegion {
                    track: *track_id,
                    region: Region {
                        id: *id,
                        start: *start,
                        length: *length,
                        loop_length: 0,
                        content: RegionContent::Clip {
                            asset: asset.clone(),
                            offset,
                            gain,
                        },
                    },
                }])
            }
            Command::SetClipGain { region: id, gain } => {
                let (_, r) = region(p, *id)?;
                let RegionContent::Clip { gain: old, .. } = &r.content else {
                    return Err(CommandError::WrongKind {
                        reason: "only audio clips with a source have a gain",
                    });
                };
                if *gain > MAX_VOLUME {
                    return Err(invalid("gain", "must be 0 to 2000"));
                }
                Ok(set(
                    FieldKey::ClipGain { region: *id },
                    FieldValue::Permille(*old),
                    FieldValue::Permille(*gain),
                ))
            }
            Command::SetClipOffset { region: id, offset } => {
                let (_, r) = region(p, *id)?;
                let RegionContent::Clip { offset: old, .. } = &r.content else {
                    return Err(CommandError::WrongKind {
                        reason: "only audio clips with a source have an offset",
                    });
                };
                if *offset > MAX_CLIP_OFFSET {
                    return Err(invalid("offset", "out of range"));
                }
                Ok(set(
                    FieldKey::ClipOffset { region: *id },
                    FieldValue::Frames(*old),
                    FieldValue::Frames(*offset),
                ))
            }
            Command::MoveRegion { region: id, start } => {
                check_tick("start", *start)?;
                let (_, r) = region(p, *id)?;
                Ok(set(
                    FieldKey::RegionStart { region: *id },
                    FieldValue::Tick(r.start),
                    FieldValue::Tick(*start),
                ))
            }
            Command::ResizeRegion { region: id, length } => {
                check_span("length", *length)?;
                let (_, r) = region(p, *id)?;
                if let RegionContent::Midi { notes } = &r.content {
                    if notes.values().any(|n| n.start + n.duration > *length) {
                        return Err(CommandError::OutOfBounds {
                            field: "length",
                            reason: "a note would end after the clip",
                        });
                    }
                }
                Ok(set(
                    FieldKey::RegionLength { region: *id },
                    FieldValue::Tick(r.length),
                    FieldValue::Tick(*length),
                ))
            }
            Command::DeleteRegion { region: id } => {
                let (t, r) = region(p, *id)?;
                Ok(vec![Change::DeleteRegion {
                    track: t.id,
                    region: r.clone(),
                }])
            }
            Command::AddNote {
                id,
                region: rid,
                pitch,
                velocity,
                start,
                duration,
            } => {
                fresh(p, *id)?;
                let (_, r) = region(p, *rid)?;
                if r.kind() != RegionKind::Midi {
                    return Err(CommandError::WrongKind {
                        reason: "notes live in midi clips",
                    });
                }
                check_pitch(*pitch)?;
                check_velocity(*velocity)?;
                check_tick("start", *start)?;
                check_span("duration", *duration)?;
                check_inside(*start, *duration, r.length)?;
                let n = Note {
                    id: *id,
                    pitch: *pitch,
                    velocity: *velocity,
                    start: *start,
                    duration: *duration,
                };
                Ok(vec![Change::NewNote {
                    region: *rid,
                    note: n,
                }])
            }
            Command::MoveNote {
                region: rid,
                note: nid,
                start,
                pitch,
            } => {
                check_tick("start", *start)?;
                check_pitch(*pitch)?;
                let n = note(p, *rid, *nid)?;
                check_inside(*start, n.duration, region(p, *rid)?.1.length)?;
                let mut out = set(
                    FieldKey::NoteStart {
                        region: *rid,
                        note: *nid,
                    },
                    FieldValue::Tick(n.start),
                    FieldValue::Tick(*start),
                );
                out.extend(set(
                    FieldKey::NotePitch {
                        region: *rid,
                        note: *nid,
                    },
                    FieldValue::Byte(n.pitch),
                    FieldValue::Byte(*pitch),
                ));
                Ok(out)
            }
            Command::ResizeNote {
                region: rid,
                note: nid,
                duration,
            } => {
                check_span("duration", *duration)?;
                let n = note(p, *rid, *nid)?;
                check_inside(n.start, *duration, region(p, *rid)?.1.length)?;
                Ok(set(
                    FieldKey::NoteDuration {
                        region: *rid,
                        note: *nid,
                    },
                    FieldValue::Tick(n.duration),
                    FieldValue::Tick(*duration),
                ))
            }
            Command::SetNoteVelocity {
                region: rid,
                note: nid,
                velocity,
            } => {
                check_velocity(*velocity)?;
                let n = note(p, *rid, *nid)?;
                Ok(set(
                    FieldKey::NoteVelocity {
                        region: *rid,
                        note: *nid,
                    },
                    FieldValue::Byte(n.velocity),
                    FieldValue::Byte(*velocity),
                ))
            }
            Command::DeleteNote {
                region: rid,
                note: nid,
            } => {
                let n = note(p, *rid, *nid)?;
                Ok(vec![Change::DeleteNote {
                    region: *rid,
                    note: n.clone(),
                }])
            }
            Command::MoveRegionToTrack {
                region: id,
                track: to,
            } => {
                let (from, r) = region(p, *id)?;
                let dest = track(p, *to)?;
                let fits = matches!(
                    (r.kind(), dest.kind),
                    (RegionKind::Midi, TrackKind::Instrument)
                        | (RegionKind::Audio, TrackKind::Audio)
                );
                if !fits {
                    return Err(CommandError::WrongKind {
                        reason: "midi clips go on instrument tracks, audio clips on audio tracks",
                    });
                }
                if from.id == *to {
                    return Ok(Vec::new());
                }
                Ok(vec![
                    Change::DeleteRegion {
                        track: from.id,
                        region: r.clone(),
                    },
                    Change::NewRegion {
                        track: *to,
                        region: r.clone(),
                    },
                ])
            }
            Command::SetTrackVolume { track: id, volume } => {
                if *volume > MAX_VOLUME {
                    return Err(invalid("volume", "must be 0 to 2000"));
                }
                mix_change(p, *id, |m| m.volume = *volume)
            }
            Command::SetTrackPan { track: id, pan } => {
                if !(-MAX_PAN..=MAX_PAN).contains(pan) {
                    return Err(invalid("pan", "must be -1000 to 1000"));
                }
                mix_change(p, *id, |m| m.pan = *pan)
            }
            Command::SetTrackMute { track: id, mute } => mix_change(p, *id, |m| m.mute = *mute),
            Command::SetTrackSolo { track: id, solo } => mix_change(p, *id, |m| m.solo = *solo),
            Command::SetMasterVolume { volume } => {
                if *volume > MAX_VOLUME {
                    return Err(invalid("volume", "must be 0 to 2000"));
                }
                Ok(set(
                    FieldKey::Master,
                    FieldValue::Master(p.master),
                    FieldValue::Master(MasterMix {
                        volume: *volume,
                        ..p.master
                    }),
                ))
            }
            Command::SetMasterReverb { size, mix } => {
                if *size > MAX_REVERB {
                    return Err(invalid("size", "must be 0 to 1000"));
                }
                if *mix > MAX_REVERB {
                    return Err(invalid("mix", "must be 0 to 1000"));
                }
                Ok(set(
                    FieldKey::Master,
                    FieldValue::Master(p.master),
                    FieldValue::Master(MasterMix {
                        reverb_size: *size,
                        reverb_mix: *mix,
                        ..p.master
                    }),
                ))
            }
            Command::SetTrackReverbSend { track: id, send } => {
                if *send > MAX_REVERB {
                    return Err(invalid("send", "must be 0 to 1000"));
                }
                let t = track(p, *id)?;
                Ok(set(
                    FieldKey::TrackReverbSend { track: *id },
                    FieldValue::Permille(t.reverb_send),
                    FieldValue::Permille(*send),
                ))
            }
            Command::AddAutomationPoint {
                target,
                tick,
                value,
                curve,
            } => {
                check_target(p, target)?;
                check_tick("tick", *tick)?;
                check_value(target, *value)?;
                if p.automation_point(target, *tick).is_some() {
                    return Err(invalid("tick", "a point already exists at this tick"));
                }
                if p.lane(target)
                    .is_some_and(|l| l.points.len() >= MAX_AUTOMATION_POINTS)
                {
                    return Err(invalid("tick", "too many points in this lane"));
                }
                Ok(vec![Change::SetAutomationPoint {
                    target: *target,
                    tick: *tick,
                    old: None,
                    new: Some(AutomationPoint {
                        value: *value,
                        curve: curve.unwrap_or_default(),
                    }),
                }])
            }
            Command::MoveAutomationPoint {
                target,
                tick,
                new_tick,
                value,
            } => {
                check_target(p, target)?;
                check_tick("new_tick", *new_tick)?;
                check_value(target, *value)?;
                let old = point_at(p, target, *tick)?;
                let new = AutomationPoint {
                    value: *value,
                    curve: old.curve,
                };
                if new_tick == tick {
                    if new == old {
                        return Ok(Vec::new());
                    }
                    return Ok(vec![Change::SetAutomationPoint {
                        target: *target,
                        tick: *tick,
                        old: Some(old),
                        new: Some(new),
                    }]);
                }
                if p.automation_point(target, *new_tick).is_some() {
                    return Err(invalid("new_tick", "a point already exists at this tick"));
                }
                Ok(vec![
                    Change::SetAutomationPoint {
                        target: *target,
                        tick: *tick,
                        old: Some(old),
                        new: None,
                    },
                    Change::SetAutomationPoint {
                        target: *target,
                        tick: *new_tick,
                        old: None,
                        new: Some(new),
                    },
                ])
            }
            Command::SetAutomationCurve {
                target,
                tick,
                curve,
            } => {
                check_target(p, target)?;
                let old = point_at(p, target, *tick)?;
                if old.curve == *curve {
                    return Ok(Vec::new());
                }
                Ok(vec![Change::SetAutomationPoint {
                    target: *target,
                    tick: *tick,
                    old: Some(old),
                    new: Some(AutomationPoint {
                        curve: *curve,
                        ..old
                    }),
                }])
            }
            Command::DeleteAutomationPoint { target, tick } => {
                check_target(p, target)?;
                let old = point_at(p, target, *tick)?;
                Ok(vec![Change::SetAutomationPoint {
                    target: *target,
                    tick: *tick,
                    old: Some(old),
                    new: None,
                }])
            }
            Command::ClearAutomationLane { target } => {
                check_target(p, target)?;
                let lane = p
                    .lane(target)
                    .ok_or_else(|| invalid("target", "no automation lane for this target"))?;
                Ok(clear_lane(target, lane))
            }
            Command::AddNotes { region: rid, notes } => {
                let (_, r) = region(p, *rid)?;
                midi_notes(r)?;
                if notes.is_empty() || notes.len() > MAX_BULK_NOTES {
                    return Err(invalid("notes", "between 1 and the bulk limit notes"));
                }
                fresh_all(p, notes.iter().map(|n| n.id))?;
                let mut out = Vec::with_capacity(notes.len());
                for n in notes {
                    check_pitch(n.pitch)?;
                    check_velocity(n.velocity)?;
                    check_tick("start", n.start)?;
                    check_span("duration", n.duration)?;
                    check_inside(n.start, n.duration, r.length)?;
                    out.push(Change::NewNote {
                        region: *rid,
                        note: Note {
                            id: n.id,
                            pitch: n.pitch,
                            velocity: n.velocity,
                            start: n.start,
                            duration: n.duration,
                        },
                    });
                }
                Ok(out)
            }
            Command::SplitRegion {
                region: rid,
                at,
                new_region,
                note_ids,
            } => {
                let (t, r) = region(p, *rid)?;
                check_tick("at", *at)?;
                let cut = *at - r.start;
                if cut <= 0 || cut >= r.length {
                    return Err(invalid("at", "must fall strictly inside the clip"));
                }
                if r.loop_length != 0 {
                    return Err(invalid(
                        "region",
                        "clear the loop before splitting the clip",
                    ));
                }
                fresh_all(
                    p,
                    std::iter::once(*new_region).chain(note_ids.iter().copied()),
                )?;
                let right_content = match &r.content {
                    RegionContent::Audio => RegionContent::Audio,
                    RegionContent::Clip { .. } => {
                        return Err(CommandError::WrongKind {
                            reason: "audio clips with a source cannot be split yet",
                        })
                    }
                    RegionContent::Midi { .. } => RegionContent::Midi {
                        notes: BTreeMap::new(),
                    },
                };
                let mut out = vec![Change::NewRegion {
                    track: t.id,
                    region: Region {
                        id: *new_region,
                        start: *at,
                        length: r.length - cut,
                        loop_length: 0,
                        content: right_content,
                    },
                }];
                let mut spare = note_ids.iter();
                if let RegionContent::Midi { notes } = &r.content {
                    for n in notes.values() {
                        let end = n.start + n.duration;
                        if end <= cut {
                            continue;
                        }
                        if n.start >= cut {
                            out.push(Change::DeleteNote {
                                region: *rid,
                                note: n.clone(),
                            });
                            out.push(Change::NewNote {
                                region: *new_region,
                                note: Note {
                                    start: n.start - cut,
                                    ..n.clone()
                                },
                            });
                        } else {
                            let id = *spare.next().ok_or(invalid(
                                "note_ids",
                                "needs one id per note crossing the cut",
                            ))?;
                            out.push(Change::Set {
                                key: FieldKey::NoteDuration {
                                    region: *rid,
                                    note: n.id,
                                },
                                old: FieldValue::Tick(n.duration),
                                new: FieldValue::Tick(cut - n.start),
                            });
                            out.push(Change::NewNote {
                                region: *new_region,
                                note: Note {
                                    id,
                                    start: 0,
                                    duration: end - cut,
                                    ..n.clone()
                                },
                            });
                        }
                    }
                }
                if spare.next().is_some() {
                    return Err(invalid("note_ids", "more ids than notes crossing the cut"));
                }
                // The left clip shrinks last, once every note left of the cut fits.
                out.push(Change::Set {
                    key: FieldKey::RegionLength { region: *rid },
                    old: FieldValue::Tick(r.length),
                    new: FieldValue::Tick(cut),
                });
                Ok(out)
            }
            Command::DuplicateRegion {
                region: rid,
                new_region,
                start,
                track: to,
                note_ids,
            } => {
                let (from, r) = region(p, *rid)?;
                check_tick("start", *start)?;
                let dest = match to {
                    Some(id) => track(p, *id)?,
                    None => from,
                };
                let fits = matches!(
                    (r.kind(), dest.kind),
                    (RegionKind::Midi, TrackKind::Instrument)
                        | (RegionKind::Audio, TrackKind::Audio)
                );
                if !fits {
                    return Err(CommandError::WrongKind {
                        reason: "midi clips go on instrument tracks, audio clips on audio tracks",
                    });
                }
                fresh_all(
                    p,
                    std::iter::once(*new_region).chain(note_ids.iter().copied()),
                )?;
                let content = match &r.content {
                    RegionContent::Audio => RegionContent::Audio,
                    RegionContent::Clip { .. } => r.content.clone(),
                    RegionContent::Midi { notes } => {
                        if notes.len() != note_ids.len() {
                            return Err(invalid("note_ids", "needs one id per note in the clip"));
                        }
                        RegionContent::Midi {
                            notes: notes
                                .values()
                                .zip(note_ids)
                                .map(|(n, id)| {
                                    (
                                        *id,
                                        Note {
                                            id: *id,
                                            ..n.clone()
                                        },
                                    )
                                })
                                .collect(),
                        }
                    }
                };
                Ok(vec![Change::NewRegion {
                    track: dest.id,
                    region: Region {
                        id: *new_region,
                        start: *start,
                        length: r.length,
                        loop_length: r.loop_length,
                        content,
                    },
                }])
            }
            Command::TransposeNotes {
                region: rid,
                semitones,
                notes: ids,
            } => {
                let (_, r) = region(p, *rid)?;
                let picked = pick_notes(midi_notes(r)?, ids)?;
                if !(-127..=127).contains(semitones) {
                    return Err(invalid("semitones", "must be -127 to 127"));
                }
                let mut out = Vec::new();
                for n in picked {
                    let to = i32::from(n.pitch) + i32::from(*semitones);
                    let Ok(to) = u8::try_from(to) else {
                        return Err(invalid("semitones", "would move a note outside 0 to 127"));
                    };
                    check_pitch(to)
                        .map_err(|_| invalid("semitones", "would move a note outside 0 to 127"))?;
                    out.extend(set(
                        FieldKey::NotePitch {
                            region: *rid,
                            note: n.id,
                        },
                        FieldValue::Byte(n.pitch),
                        FieldValue::Byte(to),
                    ));
                }
                Ok(out)
            }
            Command::QuantizeNotes {
                region: rid,
                grid,
                strength,
                notes: ids,
            } => {
                let (_, r) = region(p, *rid)?;
                let picked = pick_notes(midi_notes(r)?, ids)?;
                check_span("grid", *grid)?;
                if !(1..=100).contains(strength) {
                    return Err(invalid("strength", "must be 1 to 100"));
                }
                let mut out = Vec::new();
                for n in picked {
                    let target = (n.start + grid / 2) / grid * grid;
                    let moved = n.start + (target - n.start) * i64::from(*strength) / 100;
                    let start = moved.min(r.length - n.duration);
                    out.extend(set(
                        FieldKey::NoteStart {
                            region: *rid,
                            note: n.id,
                        },
                        FieldValue::Tick(n.start),
                        FieldValue::Tick(start),
                    ));
                }
                Ok(out)
            }
            Command::MoveNotes {
                region: rid,
                notes: ids,
                delta_tick,
                delta_pitch,
            } => {
                let (_, r) = region(p, *rid)?;
                let picked = pick_listed(midi_notes(r)?, ids)?;
                if !(-MAX_TICK..=MAX_TICK).contains(delta_tick) {
                    return Err(invalid("delta_tick", "out of range"));
                }
                if !(-127..=127).contains(delta_pitch) {
                    return Err(invalid("delta_pitch", "must be -127 to 127"));
                }
                let mut out = Vec::with_capacity(picked.len() * 2);
                for n in picked {
                    let start = n.start + delta_tick;
                    check_tick("delta_tick", start)?;
                    check_inside(start, n.duration, r.length)?;
                    let to = i32::from(n.pitch) + i32::from(*delta_pitch);
                    let to = u8::try_from(to)
                        .ok()
                        .filter(|v| *v <= 127)
                        .ok_or(invalid("delta_pitch", "would move a note outside 0 to 127"))?;
                    out.extend(set(
                        FieldKey::NoteStart {
                            region: *rid,
                            note: n.id,
                        },
                        FieldValue::Tick(n.start),
                        FieldValue::Tick(start),
                    ));
                    out.extend(set(
                        FieldKey::NotePitch {
                            region: *rid,
                            note: n.id,
                        },
                        FieldValue::Byte(n.pitch),
                        FieldValue::Byte(to),
                    ));
                }
                Ok(out)
            }
            Command::ResizeNotes {
                region: rid,
                notes: ids,
                delta,
            } => {
                let (_, r) = region(p, *rid)?;
                let picked = pick_listed(midi_notes(r)?, ids)?;
                if !(-MAX_TICK..=MAX_TICK).contains(delta) {
                    return Err(invalid("delta", "out of range"));
                }
                let mut out = Vec::with_capacity(picked.len());
                for n in picked {
                    let duration = n.duration + delta;
                    check_span("delta", duration)?;
                    check_inside(n.start, duration, r.length)?;
                    out.extend(set(
                        FieldKey::NoteDuration {
                            region: *rid,
                            note: n.id,
                        },
                        FieldValue::Tick(n.duration),
                        FieldValue::Tick(duration),
                    ));
                }
                Ok(out)
            }
            Command::SetNotesVelocity {
                region: rid,
                notes: ids,
                value,
                ramp,
            } => {
                let (_, r) = region(p, *rid)?;
                let picked = pick_listed(midi_notes(r)?, ids)?;
                let (from, to) = match (value, ramp) {
                    (Some(v), None) => (*v, *v),
                    (None, Some(rp)) => (rp.from, rp.to),
                    _ => return Err(invalid("value", "give exactly one of value and ramp")),
                };
                check_velocity(from)?;
                check_velocity(to)?;
                let first = picked.iter().map(|n| n.start).min().unwrap_or(0);
                let last = picked.iter().map(|n| n.start).max().unwrap_or(0);
                let span = last - first;
                let mut out = Vec::with_capacity(picked.len());
                for n in picked {
                    let v = if span == 0 {
                        from
                    } else {
                        let step = (i64::from(to) - i64::from(from)) * (n.start - first) / span;
                        (i64::from(from) + step) as u8
                    };
                    out.extend(set(
                        FieldKey::NoteVelocity {
                            region: *rid,
                            note: n.id,
                        },
                        FieldValue::Byte(n.velocity),
                        FieldValue::Byte(v),
                    ));
                }
                Ok(out)
            }
            Command::DeleteNotes {
                region: rid,
                notes: ids,
            } => {
                let (_, r) = region(p, *rid)?;
                Ok(pick_listed(midi_notes(r)?, ids)?
                    .into_iter()
                    .map(|n| Change::DeleteNote {
                        region: *rid,
                        note: n.clone(),
                    })
                    .collect())
            }
            Command::AddEffect {
                id,
                owner,
                kind,
                index,
            } => {
                fresh(p, *id)?;
                if let Some(t) = owner.track() {
                    if track(p, t)?.kind != TrackKind::Instrument {
                        return Err(CommandError::WrongKind {
                            reason: "only instrument tracks and the master run effects",
                        });
                    }
                }
                let len = p.chain(owner).len();
                if len >= MAX_CHAIN_EFFECTS {
                    return Err(invalid("owner", "the chain is full"));
                }
                let index = index.unwrap_or(len);
                if index > len {
                    return Err(invalid("index", "past the end of the chain"));
                }
                Ok(vec![Change::InsertEffect {
                    owner: *owner,
                    index,
                    effect: Effect::new(*id, *kind),
                }])
            }
            Command::RemoveEffect { effect: id } => {
                let (owner, index, e) = effect(p, *id)?;
                Ok(vec![Change::RemoveEffect {
                    owner,
                    index,
                    effect: e.clone(),
                }])
            }
            Command::MoveEffect { effect: id, index } => {
                let (owner, from, _) = effect(p, *id)?;
                if *index >= p.chain(&owner).len() {
                    return Err(invalid("index", "out of range"));
                }
                if *index == from {
                    return Ok(Vec::new());
                }
                Ok(vec![Change::MoveEffect {
                    owner,
                    from,
                    to: *index,
                }])
            }
            Command::SetEffectBypass { effect: id, bypass } => {
                let (_, _, e) = effect(p, *id)?;
                if e.bypass == *bypass {
                    return Ok(Vec::new());
                }
                Ok(vec![Change::SetEffectBypass {
                    effect: *id,
                    old: e.bypass,
                    new: *bypass,
                }])
            }
            Command::SetEffectParam {
                effect: id,
                param,
                value,
            } => {
                let (_, _, e) = effect(p, *id)?;
                let at = e
                    .kind
                    .param_index(param)
                    .ok_or_else(|| invalid("param", "unknown parameter of this device"))?;
                let spec = e.kind.params()[at];
                let new = i16::try_from(*value)
                    .ok()
                    .filter(|v| (spec.min..=spec.max).contains(v))
                    .ok_or_else(|| invalid("value", "outside the range of this parameter"))?;
                if e.params[at] == new {
                    return Ok(Vec::new());
                }
                Ok(vec![Change::SetEffectParam {
                    effect: *id,
                    param: at as u8,
                    old: e.params[at],
                    new,
                }])
            }
            Command::ConformNotesToScale {
                region: rid,
                notes: ids,
            } => {
                if p.tempo_map.keys.is_empty() {
                    return Err(invalid("key", "set a project key first"));
                }
                let (_, r) = region(p, *rid)?;
                let picked = pick_notes(midi_notes(r)?, ids)?;
                let mut out = Vec::new();
                for n in picked {
                    let at = r.start + n.start;
                    let Some((_, key)) = p.tempo_map.keys.range(..=at).next_back() else {
                        continue;
                    };
                    let Some(to) = nearest_in_key(key, n.pitch) else {
                        continue;
                    };
                    out.extend(set(
                        FieldKey::NotePitch {
                            region: *rid,
                            note: n.id,
                        },
                        FieldValue::Byte(n.pitch),
                        FieldValue::Byte(to),
                    ));
                }
                Ok(out)
            }
        }
    }
}
