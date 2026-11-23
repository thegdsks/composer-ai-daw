use crate::automation::{AutomationPoint, AutomationTarget, MAX_AUTOMATION_POINTS};
use crate::effects::{Effect, EffectOwner, MAX_CHAIN_EFFECTS};
use crate::id::Id;
use crate::project::{
    ChannelMix, MasterMix, Note, Project, Region, RegionContent, Track, TrackKind,
};
use crate::rules::{self, Rule};
use crate::time::{Tick, TimeSignature};
use composer_theory::Key;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt;

/// Which property a `Set` change writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum FieldKey {
    ProjectName,
    /// Master volume and reverb (D26).
    Master,
    /// Reverb send of one track in permille (D26).
    TrackReverbSend {
        track: Id,
    },
    TrackName {
        track: Id,
    },
    TrackInstrument {
        track: Id,
    },
    TrackMix {
        track: Id,
    },
    TrackSound {
        track: Id,
    },
    RegionStart {
        region: Id,
    },
    RegionLength {
        region: Id,
    },
    /// Loop length of a midi clip in ticks, 0 for none (D24).
    RegionLoop {
        region: Id,
    },
    /// Gain of an audio clip in permille (D36).
    ClipGain {
        region: Id,
    },
    /// First source frame of an audio clip (D36).
    ClipOffset {
        region: Id,
    },
    NotePitch {
        region: Id,
        note: Id,
    },
    NoteVelocity {
        region: Id,
        note: Id,
    },
    NoteStart {
        region: Id,
        note: Id,
    },
    NoteDuration {
        region: Id,
        note: Id,
    },
}

/// Value carried by a `Set` change. The variant must match the field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum FieldValue {
    Text(String),
    Slot(Option<Id>),
    Mix(ChannelMix),
    Master(MasterMix),
    /// A value in permille.
    Permille(u16),
    Sound(Option<String>),
    Tick(Tick),
    Byte(u8),
    /// A count of source frames.
    Frames(u64),
}

/// One low level, invertible edit: new, delete or set. Applying
/// `change.inverse()` undoes `change` exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Change {
    NewTrack {
        index: usize,
        track: Track,
    },
    DeleteTrack {
        index: usize,
        track: Track,
    },
    NewRegion {
        track: Id,
        region: Region,
    },
    DeleteRegion {
        track: Id,
        region: Region,
    },
    NewNote {
        region: Id,
        note: Note,
    },
    DeleteNote {
        region: Id,
        note: Note,
    },
    Set {
        key: FieldKey,
        old: FieldValue,
        new: FieldValue,
    },
    SetTempo {
        tick: Tick,
        old: Option<u32>,
        new: Option<u32>,
    },
    SetMeter {
        tick: Tick,
        old: Option<TimeSignature>,
        new: Option<TimeSignature>,
    },
    SetKey {
        tick: Tick,
        old: Option<Key>,
        new: Option<Key>,
    },
    /// Adds, renames or removes the marker at a tick (D24).
    SetMarker {
        tick: Tick,
        old: Option<String>,
        new: Option<String>,
    },
    /// Adds, replaces or removes one automation point (D36).
    SetAutomationPoint {
        target: AutomationTarget,
        tick: Tick,
        old: Option<AutomationPoint>,
        new: Option<AutomationPoint>,
    },
    /// Puts a device into a chain at a position (D45).
    InsertEffect {
        owner: EffectOwner,
        index: usize,
        effect: Effect,
    },
    /// Takes the device at a position out of a chain (D45).
    RemoveEffect {
        owner: EffectOwner,
        index: usize,
        effect: Effect,
    },
    /// Moves a device inside its chain (D45).
    MoveEffect {
        owner: EffectOwner,
        from: usize,
        to: usize,
    },
    SetEffectBypass {
        effect: Id,
        old: bool,
        new: bool,
    },
    SetEffectParam {
        effect: Id,
        param: u8,
        old: i16,
        new: i16,
    },
}

impl Change {
    pub fn inverse(&self) -> Change {
        match self.clone() {
            Change::NewTrack { index, track } => Change::DeleteTrack { index, track },
            Change::DeleteTrack { index, track } => Change::NewTrack { index, track },
            Change::NewRegion { track, region } => Change::DeleteRegion { track, region },
            Change::DeleteRegion { track, region } => Change::NewRegion { track, region },
            Change::NewNote { region, note } => Change::DeleteNote { region, note },
            Change::DeleteNote { region, note } => Change::NewNote { region, note },
            Change::Set { key, old, new } => Change::Set {
                key,
                old: new,
                new: old,
            },
            Change::SetTempo { tick, old, new } => Change::SetTempo {
                tick,
                old: new,
                new: old,
            },
            Change::SetMeter { tick, old, new } => Change::SetMeter {
                tick,
                old: new,
                new: old,
            },
            Change::SetKey { tick, old, new } => Change::SetKey {
                tick,
                old: new,
                new: old,
            },
            Change::SetMarker { tick, old, new } => Change::SetMarker {
                tick,
                old: new,
                new: old,
            },
            Change::SetAutomationPoint {
                target,
                tick,
                old,
                new,
            } => Change::SetAutomationPoint {
                target,
                tick,
                old: new,
                new: old,
            },
            Change::InsertEffect {
                owner,
                index,
                effect,
            } => Change::RemoveEffect {
                owner,
                index,
                effect,
            },
            Change::RemoveEffect {
                owner,
                index,
                effect,
            } => Change::InsertEffect {
                owner,
                index,
                effect,
            },
            Change::MoveEffect { owner, from, to } => Change::MoveEffect {
                owner,
                from: to,
                to: from,
            },
            Change::SetEffectBypass { effect, old, new } => Change::SetEffectBypass {
                effect,
                old: new,
                new: old,
            },
            Change::SetEffectParam {
                effect,
                param,
                old,
                new,
            } => Change::SetEffectParam {
                effect,
                param,
                old: new,
                new: old,
            },
        }
    }
}

/// Why a change could not be applied. The project is untouched when this is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeError {
    Missing(&'static str),
    Duplicate(Id),
    /// The recorded old value no longer matches the project.
    Stale(&'static str),
    BadIndex(usize),
    WrongValueType,
    WrongContent,
    ProtectedOrigin,
    /// A value is outside its allowed range (pitch, velocity, tick, tempo, name length...).
    OutOfRange(&'static str),
    /// A note would not lie inside its region.
    OutOfBounds(&'static str),
}

impl fmt::Display for ChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChangeError::Missing(what) => write!(f, "{what} not found"),
            ChangeError::Duplicate(id) => write!(f, "id {id} already in use"),
            ChangeError::Stale(what) => write!(f, "{what} differs from the recorded old value"),
            ChangeError::BadIndex(i) => write!(f, "track index {i} out of range"),
            ChangeError::WrongValueType => f.write_str("value type does not match the field"),
            ChangeError::WrongContent => {
                f.write_str("region content kind does not allow this change")
            }
            ChangeError::ProtectedOrigin => f.write_str("the entry at tick 0 cannot be removed"),
            ChangeError::OutOfRange(what) => write!(f, "{what} is out of range"),
            ChangeError::OutOfBounds(what) => write!(f, "{what} must stay inside its region"),
        }
    }
}

impl std::error::Error for ChangeError {}

impl From<Rule> for ChangeError {
    fn from(r: Rule) -> Self {
        match r {
            Rule::NoteOutside => ChangeError::OutOfBounds("note"),
            Rule::ContentKind => ChangeError::WrongContent,
            other => ChangeError::OutOfRange(other.what()),
        }
    }
}

fn check_name(name: &str) -> Result<(), ChangeError> {
    Ok(rules::name(name)?)
}

fn clip_only(r: &Region) -> Result<(), ChangeError> {
    match r.content {
        RegionContent::Clip { .. } => Ok(()),
        _ => Err(ChangeError::WrongContent),
    }
}

fn check_note_fields(n: &Note) -> Result<(), ChangeError> {
    Ok(rules::note_fields(n)?)
}

fn check_inside(start: Tick, duration: Tick, region_length: Tick) -> Result<(), ChangeError> {
    Ok(rules::inside(start, duration, region_length)?)
}

/// Validates a whole region (as carried by `NewRegion` and `NewTrack`) and records its ids in `seen`.
fn check_region(
    p: &Project,
    r: &Region,
    track_kind: TrackKind,
    seen: &mut HashSet<Id>,
) -> Result<(), ChangeError> {
    if p.contains_id(r.id) || !seen.insert(r.id) {
        return Err(ChangeError::Duplicate(r.id));
    }
    rules::region_fields(r)?;
    rules::content_fits(&r.content, track_kind)?;
    if let RegionContent::Midi { notes } = &r.content {
        for (key, n) in notes {
            if *key != n.id || p.contains_id(n.id) || !seen.insert(n.id) {
                return Err(ChangeError::Duplicate(n.id));
            }
            check_note_fields(n)?;
            check_inside(n.start, n.duration, r.length)?;
        }
    }
    Ok(())
}

impl Project {
    /// Applies one change. On error nothing has been modified. Ranges and the
    /// notes-inside-region rule are enforced here, so no client path can bypass them.
    pub fn apply(&mut self, change: &Change) -> Result<(), ChangeError> {
        match change {
            Change::NewTrack { index, track } => {
                if *index > self.tracks.len() {
                    return Err(ChangeError::BadIndex(*index));
                }
                if self.contains_id(track.id) {
                    return Err(ChangeError::Duplicate(track.id));
                }
                check_name(&track.name)?;
                rules::mix(&track.mix)?;
                let mut seen = HashSet::from([track.id]);
                for r in track.regions.values() {
                    check_region(self, r, track.kind, &mut seen)?;
                }
                self.insert_track(*index, track.clone());
            }
            Change::DeleteTrack { index, track } => {
                match self.tracks.get(*index) {
                    Some(t) if t.id == track.id => {}
                    Some(_) => return Err(ChangeError::Stale("track position")),
                    None => return Err(ChangeError::BadIndex(*index)),
                }
                // Lanes of the track must be cleared first, so undo can bring them back (D36).
                if self
                    .automation
                    .iter()
                    .any(|l| l.target.track() == Some(track.id))
                {
                    return Err(ChangeError::OutOfRange("automation of a deleted track"));
                }
                if !self
                    .chain(&EffectOwner::Track { track: track.id })
                    .is_empty()
                {
                    return Err(ChangeError::OutOfRange("effects of a deleted track"));
                }
                self.remove_track_at(*index);
            }
            Change::NewRegion { track, region } => {
                if self.contains_id(region.id) {
                    return Err(ChangeError::Duplicate(region.id));
                }
                let kind = self
                    .track(*track)
                    .ok_or(ChangeError::Missing("track"))?
                    .kind;
                check_region(self, region, kind, &mut HashSet::new())?;
                self.track_mut(*track)
                    .ok_or(ChangeError::Missing("track"))?
                    .regions
                    .insert(region.id, region.clone());
                self.index_region(*track, region);
            }
            Change::DeleteRegion { track, region } => {
                let t = self
                    .track_mut(*track)
                    .ok_or(ChangeError::Missing("track"))?;
                let removed = t
                    .regions
                    .remove(&region.id)
                    .ok_or(ChangeError::Missing("region"))?;
                self.unindex_region(&removed);
            }
            Change::NewNote { region, note } => {
                if self.contains_id(note.id) {
                    return Err(ChangeError::Duplicate(note.id));
                }
                check_note_fields(note)?;
                let r = self
                    .region_mut(*region)
                    .ok_or(ChangeError::Missing("region"))?;
                check_inside(note.start, note.duration, r.length)?;
                match &mut r.content {
                    RegionContent::Midi { notes } => notes.insert(note.id, note.clone()),
                    RegionContent::Audio | RegionContent::Clip { .. } => {
                        return Err(ChangeError::WrongContent)
                    }
                };
                self.index_note(*region, note.id);
            }
            Change::DeleteNote { region, note } => {
                self.notes_mut(*region)?
                    .remove(&note.id)
                    .ok_or(ChangeError::Missing("note"))?;
                self.unindex_note(note.id);
            }
            Change::Set { key, old, new } => {
                if self.get(key)? != *old {
                    return Err(ChangeError::Stale("field"));
                }
                self.check_set(key, new)?;
                self.put(key, new.clone())?;
            }
            Change::SetTempo { tick, old, new } => {
                if self.tempo_map.tempos.get(tick).copied() != *old {
                    return Err(ChangeError::Stale("tempo"));
                }
                if *tick == 0 && new.is_none() {
                    return Err(ChangeError::ProtectedOrigin);
                }
                if let Some(v) = new {
                    rules::tempo_entry(*tick, *v)?;
                }
                match new {
                    Some(v) => self.tempo_map.tempos.insert(*tick, *v),
                    None => self.tempo_map.tempos.remove(tick),
                };
            }
            Change::SetMeter { tick, old, new } => {
                if self.tempo_map.meters.get(tick).copied() != *old {
                    return Err(ChangeError::Stale("meter"));
                }
                if *tick == 0 && new.is_none() {
                    return Err(ChangeError::ProtectedOrigin);
                }
                if let Some(v) = new {
                    rules::meter_entry(*tick, v)?;
                }
                match new {
                    Some(v) => self.tempo_map.meters.insert(*tick, *v),
                    None => self.tempo_map.meters.remove(tick),
                };
            }
            Change::SetKey { tick, old, new } => {
                if self.tempo_map.keys.get(tick).copied() != *old {
                    return Err(ChangeError::Stale("key"));
                }
                if let Some(v) = new {
                    rules::key_entry(*tick, v)?;
                }
                match new {
                    Some(v) => self.tempo_map.keys.insert(*tick, *v),
                    None => self.tempo_map.keys.remove(tick),
                };
            }
            Change::SetAutomationPoint {
                target,
                tick,
                old,
                new,
            } => {
                if self.automation_point(target, *tick) != *old {
                    return Err(ChangeError::Stale("automation point"));
                }
                if let Some(t) = target.track() {
                    if self.track(t).is_none() {
                        return Err(ChangeError::Missing("track"));
                    }
                }
                if let Some(p) = new {
                    rules::automation_point(target, *tick, p)?;
                    let len = self.lane(target).map_or(0, |l| l.points.len());
                    if old.is_none() && len >= MAX_AUTOMATION_POINTS {
                        return Err(ChangeError::OutOfRange("automation point count"));
                    }
                }
                self.put_automation_point(*target, *tick, *new);
            }
            Change::InsertEffect {
                owner,
                index,
                effect,
            } => {
                if let Some(t) = owner.track() {
                    if self.track(t).is_none() {
                        return Err(ChangeError::Missing("track"));
                    }
                }
                if self.contains_id(effect.id) {
                    return Err(ChangeError::Duplicate(effect.id));
                }
                rules::effect(effect)?;
                let len = self.chain(owner).len();
                if len >= MAX_CHAIN_EFFECTS {
                    return Err(ChangeError::OutOfRange("effect count"));
                }
                if *index > len {
                    return Err(ChangeError::BadIndex(*index));
                }
                self.insert_effect(*owner, *index, effect.clone());
            }
            Change::RemoveEffect {
                owner,
                index,
                effect,
            } => {
                match self.chain(owner).get(*index) {
                    Some(e) if e == effect => {}
                    Some(_) => return Err(ChangeError::Stale("effect")),
                    None => return Err(ChangeError::BadIndex(*index)),
                }
                self.remove_effect_at(*owner, *index);
            }
            Change::MoveEffect { owner, from, to } => {
                let len = self.chain(owner).len();
                if *from >= len {
                    return Err(ChangeError::BadIndex(*from));
                }
                if *to >= len {
                    return Err(ChangeError::BadIndex(*to));
                }
                let e = self.remove_effect_at(*owner, *from);
                self.insert_effect(*owner, *to, e);
            }
            Change::SetEffectBypass { effect, old, new } => {
                let e = self
                    .effect_mut(*effect)
                    .ok_or(ChangeError::Missing("effect"))?;
                if e.bypass != *old {
                    return Err(ChangeError::Stale("effect bypass"));
                }
                e.bypass = *new;
            }
            Change::SetEffectParam {
                effect,
                param,
                old,
                new,
            } => {
                let e = self
                    .effect_mut(*effect)
                    .ok_or(ChangeError::Missing("effect"))?;
                let spec = e
                    .kind
                    .params()
                    .get(usize::from(*param))
                    .ok_or(ChangeError::Missing("effect parameter"))?;
                if e.params[usize::from(*param)] != *old {
                    return Err(ChangeError::Stale("effect parameter"));
                }
                if !(spec.min..=spec.max).contains(new) {
                    return Err(ChangeError::OutOfRange("effect parameter"));
                }
                e.params[usize::from(*param)] = *new;
            }
            Change::SetMarker { tick, old, new } => {
                if self.markers.get(tick) != old.as_ref() {
                    return Err(ChangeError::Stale("marker"));
                }
                if let Some(v) = new {
                    rules::marker_entry(*tick, v)?;
                    if old.is_none() && self.markers.len() >= rules::MAX_MARKERS {
                        return Err(ChangeError::OutOfRange("marker count"));
                    }
                }
                match new {
                    Some(v) => self.markers.insert(*tick, v.clone()),
                    None => self.markers.remove(tick),
                };
            }
        }
        Ok(())
    }

    fn notes_mut(&mut self, region: Id) -> Result<&mut BTreeMap<Id, Note>, ChangeError> {
        let r = self
            .region_mut(region)
            .ok_or(ChangeError::Missing("region"))?;
        match &mut r.content {
            RegionContent::Midi { notes } => Ok(notes),
            RegionContent::Audio | RegionContent::Clip { .. } => Err(ChangeError::WrongContent),
        }
    }

    fn note_mut(&mut self, region: Id, note: Id) -> Result<&mut Note, ChangeError> {
        self.notes_mut(region)?
            .get_mut(&note)
            .ok_or(ChangeError::Missing("note"))
    }

    /// Range and bounds rules for a `Set`. A value of the wrong type passes here and fails in `put`.
    fn check_set(&self, key: &FieldKey, new: &FieldValue) -> Result<(), ChangeError> {
        use FieldValue::{Byte, Text, Tick};
        let note = |r: Id, n: Id| self.note(r, n).ok_or(ChangeError::Missing("note"));
        let region = |r: Id| {
            self.region(r)
                .map(|(_, r)| r)
                .ok_or(ChangeError::Missing("region"))
        };
        match (key, new) {
            (FieldKey::ProjectName | FieldKey::TrackName { .. }, Text(v)) => check_name(v),
            (FieldKey::TrackMix { .. }, FieldValue::Mix(m)) => Ok(rules::mix(m)?),
            (FieldKey::Master, FieldValue::Master(m)) => Ok(rules::master(m)?),
            (FieldKey::TrackReverbSend { .. }, FieldValue::Permille(v)) => {
                Ok(rules::reverb_send(*v)?)
            }
            (FieldKey::TrackSound { .. }, FieldValue::Sound(Some(s))) => Ok(rules::sound_id(s)?),
            (FieldKey::RegionStart { .. }, Tick(v)) => Ok(rules::region_start(*v)?),
            (FieldKey::RegionLength { region: r }, Tick(v)) => {
                rules::region_length(*v)?;
                if let RegionContent::Midi { notes } = &region(*r)?.content {
                    if notes.values().any(|n| n.start + n.duration > *v) {
                        return Err(ChangeError::OutOfBounds("region length"));
                    }
                }
                Ok(())
            }
            (FieldKey::RegionLoop { region: r }, Tick(v)) => {
                rules::region_loop(*v)?;
                match region(*r)?.content {
                    RegionContent::Midi { .. } => Ok(()),
                    RegionContent::Audio | RegionContent::Clip { .. } => {
                        Err(ChangeError::WrongContent)
                    }
                }
            }
            (FieldKey::ClipGain { region: r }, FieldValue::Permille(v)) => {
                rules::clip_gain(*v)?;
                clip_only(region(*r)?)
            }
            (FieldKey::ClipOffset { region: r }, FieldValue::Frames(v)) => {
                rules::clip_offset(*v)?;
                clip_only(region(*r)?)
            }
            (FieldKey::NotePitch { .. }, Byte(v)) => Ok(rules::pitch(*v)?),
            (FieldKey::NoteVelocity { .. }, Byte(v)) => Ok(rules::velocity(*v)?),
            (FieldKey::NoteStart { region: r, note: n }, Tick(v)) => {
                rules::note_start(*v)?;
                check_inside(*v, note(*r, *n)?.duration, region(*r)?.length)
            }
            (FieldKey::NoteDuration { region: r, note: n }, Tick(v)) => {
                rules::note_duration(*v)?;
                check_inside(note(*r, *n)?.start, *v, region(*r)?.length)
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn get(&self, key: &FieldKey) -> Result<FieldValue, ChangeError> {
        let track = |id: Id| self.track(id).ok_or(ChangeError::Missing("track"));
        let region = |id: Id| {
            self.region(id)
                .map(|(_, r)| r)
                .ok_or(ChangeError::Missing("region"))
        };
        let note = |r: Id, n: Id| self.note(r, n).ok_or(ChangeError::Missing("note"));
        Ok(match key {
            FieldKey::ProjectName => FieldValue::Text(self.name.clone()),
            FieldKey::Master => FieldValue::Master(self.master),
            FieldKey::TrackReverbSend { track: t } => FieldValue::Permille(track(*t)?.reverb_send),
            FieldKey::TrackName { track: t } => FieldValue::Text(track(*t)?.name.clone()),
            FieldKey::TrackInstrument { track: t } => FieldValue::Slot(track(*t)?.instrument),
            FieldKey::TrackMix { track: t } => FieldValue::Mix(track(*t)?.mix),
            FieldKey::TrackSound { track: t } => FieldValue::Sound(track(*t)?.sound.clone()),
            FieldKey::RegionStart { region: r } => FieldValue::Tick(region(*r)?.start),
            FieldKey::RegionLength { region: r } => FieldValue::Tick(region(*r)?.length),
            FieldKey::RegionLoop { region: r } => FieldValue::Tick(region(*r)?.loop_length),
            FieldKey::ClipGain { region: r } => match &region(*r)?.content {
                RegionContent::Clip { gain, .. } => FieldValue::Permille(*gain),
                _ => return Err(ChangeError::WrongContent),
            },
            FieldKey::ClipOffset { region: r } => match &region(*r)?.content {
                RegionContent::Clip { offset, .. } => FieldValue::Frames(*offset),
                _ => return Err(ChangeError::WrongContent),
            },
            FieldKey::NotePitch { region: r, note: n } => FieldValue::Byte(note(*r, *n)?.pitch),
            FieldKey::NoteVelocity { region: r, note: n } => {
                FieldValue::Byte(note(*r, *n)?.velocity)
            }
            FieldKey::NoteStart { region: r, note: n } => FieldValue::Tick(note(*r, *n)?.start),
            FieldKey::NoteDuration { region: r, note: n } => {
                FieldValue::Tick(note(*r, *n)?.duration)
            }
        })
    }

    pub(crate) fn put(&mut self, key: &FieldKey, value: FieldValue) -> Result<(), ChangeError> {
        use FieldValue::{Byte, Slot, Text, Tick};
        let missing_track = || ChangeError::Missing("track");
        let missing_region = || ChangeError::Missing("region");
        match (key, value) {
            (FieldKey::ProjectName, Text(v)) => self.name = v,
            (FieldKey::Master, FieldValue::Master(v)) => self.master = v,
            (FieldKey::TrackReverbSend { track }, FieldValue::Permille(v)) => {
                self.track_mut(*track)
                    .ok_or_else(missing_track)?
                    .reverb_send = v
            }
            (FieldKey::TrackName { track }, Text(v)) => {
                self.track_mut(*track).ok_or_else(missing_track)?.name = v
            }
            (FieldKey::TrackInstrument { track }, Slot(v)) => {
                self.track_mut(*track).ok_or_else(missing_track)?.instrument = v
            }
            (FieldKey::TrackMix { track }, FieldValue::Mix(v)) => {
                self.track_mut(*track).ok_or_else(missing_track)?.mix = v
            }
            (FieldKey::TrackSound { track }, FieldValue::Sound(v)) => {
                self.track_mut(*track).ok_or_else(missing_track)?.sound = v
            }
            (FieldKey::RegionStart { region }, Tick(v)) => {
                self.region_mut(*region).ok_or_else(missing_region)?.start = v
            }
            (FieldKey::RegionLength { region }, Tick(v)) => {
                self.region_mut(*region).ok_or_else(missing_region)?.length = v
            }
            (FieldKey::RegionLoop { region }, Tick(v)) => {
                self.region_mut(*region)
                    .ok_or_else(missing_region)?
                    .loop_length = v
            }
            (FieldKey::ClipGain { region }, FieldValue::Permille(v)) => {
                match &mut self.region_mut(*region).ok_or_else(missing_region)?.content {
                    RegionContent::Clip { gain, .. } => *gain = v,
                    _ => return Err(ChangeError::WrongContent),
                }
            }
            (FieldKey::ClipOffset { region }, FieldValue::Frames(v)) => {
                match &mut self.region_mut(*region).ok_or_else(missing_region)?.content {
                    RegionContent::Clip { offset, .. } => *offset = v,
                    _ => return Err(ChangeError::WrongContent),
                }
            }
            (FieldKey::NotePitch { region, note }, Byte(v)) => {
                self.note_mut(*region, *note)?.pitch = v
            }
            (FieldKey::NoteVelocity { region, note }, Byte(v)) => {
                self.note_mut(*region, *note)?.velocity = v
            }
            (FieldKey::NoteStart { region, note }, Tick(v)) => {
                self.note_mut(*region, *note)?.start = v
            }
            (FieldKey::NoteDuration { region, note }, Tick(v)) => {
                self.note_mut(*region, *note)?.duration = v
            }
            _ => return Err(ChangeError::WrongValueType),
        }
        Ok(())
    }
}
