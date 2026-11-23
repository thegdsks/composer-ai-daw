//! The single source of truth for what a valid value is. `Project::apply` and
//! `Project::validate` both call these, so the two paths cannot drift.

use crate::id::Id;
use crate::project::{
    ChannelMix, MasterMix, Note, Region, RegionContent, TrackKind, MAX_CLIP_OFFSET, MAX_MBPM,
    MAX_NAME_LEN, MAX_PAN, MAX_REVERB, MAX_VOLUME, MIN_MBPM,
};
use crate::time::{Tick, TimeSignature, MAX_TICK};
use composer_theory::Key;

/// Which rule a value broke. Each variant has a stable [`Rule::code`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    DuplicateId(Id),
    Name,
    Pitch,
    Velocity,
    NoteStart,
    NoteDuration,
    RegionStart,
    RegionLength,
    /// A loop length outside 0..=MAX_TICK.
    RegionLoop,
    /// A tempo or meter entry placed outside the allowed tick range.
    EntryTick,
    Tempo,
    Meter,
    /// A key with a root above 11.
    Key,
    /// Tick 0 has no tempo or no meter entry.
    OriginMissing,
    NoteOutside,
    ContentKind,
    RegionKeyMismatch,
    NoteKeyMismatch(Id),
    IndexInconsistent,
    /// Volume or pan outside its range.
    Mix,
    /// Master volume, reverb size or reverb mix, or a reverb send, outside its range.
    Bus,
    /// A sound id that is empty, too long or has characters outside a to z, 0 to 9 and hyphen.
    Sound,
    /// An audio clip with a malformed asset hash, an offset past MAX_CLIP_OFFSET or a gain past MAX_VOLUME.
    Clip,
    /// An automation point outside its target range, or a lane that is empty, unsorted, too
    /// long or owned by a track that does not exist.
    Automation,
    /// A device with the wrong parameter count, a value outside its range, or a chain that is
    /// empty, unsorted, too long or owned by a track that does not exist.
    Effect,
}

impl Rule {
    /// Stable machine readable code.
    pub fn code(&self) -> &'static str {
        match self {
            Rule::DuplicateId(_) => "duplicate_id",
            Rule::Name => "name_length",
            Rule::Pitch => "pitch_range",
            Rule::Velocity => "velocity_range",
            Rule::NoteStart => "note_start_range",
            Rule::NoteDuration => "note_duration_range",
            Rule::RegionStart => "region_start_range",
            Rule::RegionLength => "region_length_range",
            Rule::RegionLoop => "region_loop_range",
            Rule::EntryTick => "tempo_tick_range",
            Rule::Tempo => "tempo_range",
            Rule::Meter => "meter_invalid",
            Rule::Key => "key_invalid",
            Rule::OriginMissing => "tempo_origin_missing",
            Rule::NoteOutside => "note_outside_region",
            Rule::ContentKind => "content_kind_mismatch",
            Rule::RegionKeyMismatch => "region_key_mismatch",
            Rule::NoteKeyMismatch(_) => "note_key_mismatch",
            Rule::IndexInconsistent => "index_inconsistent",
            Rule::Mix => "mix_range",
            Rule::Bus => "bus_range",
            Rule::Sound => "sound_id",
            Rule::Clip => "clip_invalid",
            Rule::Automation => "automation_invalid",
            Rule::Effect => "effect_invalid",
        }
    }

    /// Short name of the offending value, as used in range errors.
    pub fn what(&self) -> &'static str {
        match self {
            Rule::Name => "name",
            Rule::Pitch => "pitch",
            Rule::Velocity => "velocity",
            Rule::NoteStart => "note start",
            Rule::NoteDuration => "note duration",
            Rule::RegionStart => "region start",
            Rule::RegionLength => "region length",
            Rule::RegionLoop => "loop length",
            Rule::EntryTick => "tick",
            Rule::Tempo => "tempo",
            Rule::Meter => "time signature",
            Rule::Key => "key",
            Rule::Mix => "mix",
            Rule::Bus => "reverb",
            Rule::Sound => "sound id",
            Rule::Clip => "clip",
            Rule::Automation => "automation",
            Rule::Effect => "effect",
            Rule::NoteOutside => "note",
            _ => "value",
        }
    }
}

fn ensure(ok: bool, rule: Rule) -> Result<(), Rule> {
    if ok {
        Ok(())
    } else {
        Err(rule)
    }
}

pub(crate) fn name(s: &str) -> Result<(), Rule> {
    ensure(s.len() <= MAX_NAME_LEN, Rule::Name)
}

pub(crate) fn pitch(v: u8) -> Result<(), Rule> {
    ensure(v <= 127, Rule::Pitch)
}

pub(crate) fn velocity(v: u8) -> Result<(), Rule> {
    ensure((1..=127).contains(&v), Rule::Velocity)
}

pub(crate) fn note_start(v: Tick) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&v), Rule::NoteStart)
}

pub(crate) fn note_duration(v: Tick) -> Result<(), Rule> {
    ensure((1..=MAX_TICK).contains(&v), Rule::NoteDuration)
}

pub(crate) fn region_start(v: Tick) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&v), Rule::RegionStart)
}

pub(crate) fn region_length(v: Tick) -> Result<(), Rule> {
    ensure((1..=MAX_TICK).contains(&v), Rule::RegionLength)
}

pub(crate) fn region_loop(v: Tick) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&v), Rule::RegionLoop)
}

/// Most markers one project may hold.
pub const MAX_MARKERS: usize = 10_000;

pub(crate) fn marker_entry(tick: Tick, label: &str) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&tick), Rule::EntryTick)?;
    name(label)
}

pub(crate) fn note_fields(n: &Note) -> Result<(), Rule> {
    pitch(n.pitch)?;
    velocity(n.velocity)?;
    note_start(n.start)?;
    note_duration(n.duration)
}

pub(crate) fn region_fields(r: &Region) -> Result<(), Rule> {
    region_start(r.start)?;
    region_length(r.length)?;
    region_loop(r.loop_length)?;
    ensure(
        r.loop_length == 0 || matches!(r.content, RegionContent::Midi { .. }),
        Rule::ContentKind,
    )?;
    if let RegionContent::Clip {
        asset,
        offset,
        gain,
    } = &r.content
    {
        clip_asset(asset)?;
        clip_offset(*offset)?;
        clip_gain(*gain)?;
    }
    Ok(())
}

/// 64 lowercase hex digits, the SHA-256 of the audio file.
pub fn clip_asset(s: &str) -> Result<(), Rule> {
    let ok = s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    ensure(ok, Rule::Clip)
}

pub(crate) fn clip_offset(v: u64) -> Result<(), Rule> {
    ensure(v <= MAX_CLIP_OFFSET, Rule::Clip)
}

pub(crate) fn clip_gain(v: u16) -> Result<(), Rule> {
    ensure(v <= MAX_VOLUME, Rule::Clip)
}

/// A note must lie inside its region. Both operands are at most MAX_TICK, so the sum cannot overflow.
pub(crate) fn inside(start: Tick, duration: Tick, region_length: Tick) -> Result<(), Rule> {
    ensure(start + duration <= region_length, Rule::NoteOutside)
}

pub(crate) fn content_fits(c: &RegionContent, kind: TrackKind) -> Result<(), Rule> {
    let ok = matches!(
        (c, kind),
        (RegionContent::Midi { .. }, TrackKind::Instrument)
            | (
                RegionContent::Audio | RegionContent::Clip { .. },
                TrackKind::Audio
            )
    );
    ensure(ok, Rule::ContentKind)
}

pub(crate) fn tempo_entry(tick: Tick, mbpm: u32) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&tick), Rule::EntryTick)?;
    ensure((MIN_MBPM..=MAX_MBPM).contains(&mbpm), Rule::Tempo)
}

pub(crate) fn meter_entry(tick: Tick, m: &TimeSignature) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&tick), Rule::EntryTick)?;
    ensure(m.is_valid(), Rule::Meter)
}

pub(crate) fn key_entry(tick: Tick, k: &Key) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&tick), Rule::EntryTick)?;
    ensure(k.is_valid(), Rule::Key)
}

pub(crate) fn master(m: &MasterMix) -> Result<(), Rule> {
    ensure(
        m.volume <= MAX_VOLUME && m.reverb_size <= MAX_REVERB && m.reverb_mix <= MAX_REVERB,
        Rule::Bus,
    )
}

pub(crate) fn reverb_send(v: u16) -> Result<(), Rule> {
    ensure(v <= MAX_REVERB, Rule::Bus)
}

pub(crate) fn mix(m: &ChannelMix) -> Result<(), Rule> {
    ensure(
        m.volume <= MAX_VOLUME && (-MAX_PAN..=MAX_PAN).contains(&m.pan),
        Rule::Mix,
    )
}

/// Longest catalogue sound id accepted, in bytes.
pub const MAX_SOUND_ID_LEN: usize = 64;

pub(crate) fn sound_id(s: &str) -> Result<(), Rule> {
    let ok = !s.is_empty()
        && s.len() <= MAX_SOUND_ID_LEN
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    ensure(ok, Rule::Sound)
}

pub(crate) fn automation_point(
    target: &crate::automation::AutomationTarget,
    tick: Tick,
    p: &crate::automation::AutomationPoint,
) -> Result<(), Rule> {
    ensure((0..=MAX_TICK).contains(&tick), Rule::EntryTick)?;
    ensure(target.accepts(p.value), Rule::Automation)
}

pub(crate) fn effect(e: &crate::effects::Effect) -> Result<(), Rule> {
    ensure(e.is_valid(), Rule::Effect)
}
