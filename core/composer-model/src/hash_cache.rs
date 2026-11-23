//! Incremental `state_hash` (D47). The value is exactly `Project::state_hash`; only the way it is
//! computed differs.
//!
//! The hash is a byte fold `h = (h ^ b) * P`. Xor with a byte touches only the low 8 bits of `h`,
//! and those evolve independently of the high bits, so for a fixed byte string the fold from any
//! start `h` equals `pow * h + table[h & 255]`. A cached segment therefore turns into an O(1)
//! step on a concrete state, and the document hash is the result of walking its segments in order.

use crate::change::{Change, FieldKey};
use crate::hash::{write_header, write_note, write_region_head, write_track_head, Fnv, MULT, SEED};
use crate::id::Id;
use crate::project::{Project, RegionContent};
use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Bound;

/// A note starts a new run when this many low bits of its mixed id are zero (64 notes per run on
/// average). Depends only on the id, so edits never move another boundary.
const RUN_BITS: u32 = 6;

fn is_boundary(id: Id) -> bool {
    let x = (id.0 as u64) ^ ((id.0 >> 64) as u64);
    let x = (x ^ (x >> 31)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (x >> 40) & ((1 << RUN_BITS) - 1) == 0
}

/// Cheap hasher for ids, which are already well spread.
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(MULT);
        }
    }
    fn write_u128(&mut self, v: u128) {
        let x = (v as u64) ^ ((v >> 64) as u64).rotate_left(29);
        self.0 = (x ^ (x >> 32)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type IdMap<V> = HashMap<Id, V, BuildHasherDefault<IdHasher>>;

#[derive(Debug)]
struct Table {
    pow: u64,
    c: [u64; 256],
}

/// One cached piece of the byte stream: a memo of the last evaluation and, once the input state
/// moved while the bytes stayed, the table that maps any input state to its output.
#[derive(Debug, Default)]
struct Seg {
    valid: bool,
    input: u64,
    output: u64,
    table: Option<Box<Table>>,
}

impl Seg {
    fn invalidate(&mut self) {
        self.valid = false;
        self.table = None;
    }

    /// Output state for input `state`; `fold` writes this segment's bytes.
    fn eval(&mut self, state: u64, fold: impl Fn(&mut Fnv)) -> u64 {
        if self.valid {
            if self.input == state {
                return self.output;
            }
            let t = self.table.get_or_insert_with(|| build(&fold));
            let out = t
                .pow
                .wrapping_mul(state)
                .wrapping_add(t.c[(state & 255) as usize]);
            self.input = state;
            self.output = out;
            return out;
        }
        let mut f = Fnv::new(state);
        fold(&mut f);
        self.valid = true;
        self.table = None;
        self.input = state;
        self.output = f.h;
        f.h
    }
}

fn build(fold: &impl Fn(&mut Fnv)) -> Box<Table> {
    let mut rec = Fnv::new(0);
    rec.rec = Some(Vec::new());
    fold(&mut rec);
    let bytes = rec.rec.unwrap_or_default();
    let mut pow = 1u64;
    for _ in 0..rec.n {
        pow = pow.wrapping_mul(MULT);
    }
    // All 256 start states advance together, byte by byte, so the multiplies overlap.
    let mut lanes = [0u64; 256];
    for (l, v) in lanes.iter_mut().enumerate() {
        *v = l as u64;
    }
    for &b in &bytes {
        let b = u64::from(b);
        for v in lanes.iter_mut() {
            *v = (*v ^ b).wrapping_mul(MULT);
        }
    }
    let mut c = [0u64; 256];
    for (l, slot) in c.iter_mut().enumerate() {
        *slot = lanes[l].wrapping_sub(pow.wrapping_mul(l as u64));
    }
    Box::new(Table { pow, c })
}

/// Segments of one midi region: the head plus the notes before the first boundary, then one run
/// per boundary note (key) up to the next boundary.
#[derive(Debug, Default)]
struct RegionSegs {
    first: Seg,
    runs: BTreeMap<Id, Seg>,
}

impl RegionSegs {
    fn scan(notes: &BTreeMap<Id, crate::project::Note>) -> Self {
        let runs = notes
            .keys()
            .filter(|id| is_boundary(**id))
            .map(|id| (*id, Seg::default()))
            .collect();
        Self {
            first: Seg::default(),
            runs,
        }
    }

    /// Marks the run that holds `note` (or the head segment when none starts at or before it).
    fn dirty_run_at_or_before(&mut self, note: Id) {
        match self.runs.range_mut(..=note).next_back() {
            Some((_, s)) => s.invalidate(),
            None => self.first.invalidate(),
        }
    }

    fn dirty_run_before(&mut self, note: Id) {
        match self.runs.range_mut(..note).next_back() {
            Some((_, s)) => s.invalidate(),
            None => self.first.invalidate(),
        }
    }
}

/// Incremental state hash. `touch` every change applied to the project (in order), then ask
/// `hash` with that project. A cache never applied to a project must be fresh (`new`).
#[derive(Debug, Default)]
pub struct HashCache {
    header: Seg,
    tracks: IdMap<Seg>,
    regions: IdMap<RegionSegs>,
    last: Option<u64>,
}

fn run_notes(
    notes: &BTreeMap<Id, crate::project::Note>,
    from: Option<Id>,
    to: Option<Id>,
) -> impl Iterator<Item = &crate::project::Note> {
    let lo = from.map_or(Bound::Unbounded, Bound::Included);
    let hi = to.map_or(Bound::Unbounded, Bound::Excluded);
    notes.range((lo, hi)).map(|(_, n)| n)
}

impl HashCache {
    /// True when a note with this id starts a new run. Exposed so tests can provoke run edges.
    pub fn starts_run(id: Id) -> bool {
        is_boundary(id)
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets everything; the next `hash` is a full pass.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Records that `c` was applied to the project this cache follows.
    pub fn touch(&mut self, c: &Change) {
        self.last = None;
        match c {
            Change::NewTrack { track, .. } | Change::DeleteTrack { track, .. } => {
                self.header.invalidate();
                self.tracks.remove(&track.id);
                for id in track.regions.keys() {
                    self.regions.remove(id);
                }
            }
            Change::NewRegion { track, region } | Change::DeleteRegion { track, region } => {
                if let Some(s) = self.tracks.get_mut(track) {
                    s.invalidate();
                }
                self.regions.remove(&region.id);
            }
            Change::NewNote { region, note } => {
                if let Some(rs) = self.regions.get_mut(region) {
                    rs.first.invalidate();
                    if is_boundary(note.id) {
                        rs.dirty_run_before(note.id);
                        rs.runs.insert(note.id, Seg::default());
                    } else {
                        rs.dirty_run_at_or_before(note.id);
                    }
                }
            }
            Change::DeleteNote { region, note } => {
                if let Some(rs) = self.regions.get_mut(region) {
                    rs.first.invalidate();
                    if is_boundary(note.id) {
                        rs.dirty_run_before(note.id);
                        rs.runs.remove(&note.id);
                    } else {
                        rs.dirty_run_at_or_before(note.id);
                    }
                }
            }
            Change::Set { key, .. } => match key {
                FieldKey::ProjectName | FieldKey::Master => self.header.invalidate(),
                FieldKey::TrackReverbSend { track }
                | FieldKey::TrackName { track }
                | FieldKey::TrackInstrument { track }
                | FieldKey::TrackMix { track }
                | FieldKey::TrackSound { track } => {
                    if let Some(s) = self.tracks.get_mut(track) {
                        s.invalidate();
                    }
                }
                FieldKey::RegionStart { region }
                | FieldKey::RegionLength { region }
                | FieldKey::RegionLoop { region }
                | FieldKey::ClipGain { region }
                | FieldKey::ClipOffset { region } => {
                    if let Some(rs) = self.regions.get_mut(region) {
                        rs.first.invalidate();
                    }
                }
                FieldKey::NotePitch { region, note }
                | FieldKey::NoteVelocity { region, note }
                | FieldKey::NoteStart { region, note }
                | FieldKey::NoteDuration { region, note } => {
                    if let Some(rs) = self.regions.get_mut(region) {
                        rs.dirty_run_at_or_before(*note);
                    }
                }
            },
            Change::SetTempo { .. }
            | Change::SetMeter { .. }
            | Change::SetKey { .. }
            | Change::SetMarker { .. }
            | Change::SetAutomationPoint { .. }
            | Change::InsertEffect { .. }
            | Change::RemoveEffect { .. }
            | Change::MoveEffect { .. }
            | Change::SetEffectBypass { .. }
            | Change::SetEffectParam { .. } => self.header.invalidate(),
        }
    }

    /// Same value as `project.state_hash()`, reusing what earlier calls computed.
    pub fn hash(&mut self, p: &Project) -> u64 {
        if let Some(v) = self.last {
            return v;
        }
        let mut s = self.header.eval(SEED, |h| write_header(p, h));
        for t in &p.tracks {
            s = self
                .tracks
                .entry(t.id)
                .or_default()
                .eval(s, |h| write_track_head(t, h));
            for r in t.regions.values() {
                let rs = self
                    .regions
                    .entry(r.id)
                    .or_insert_with(|| match &r.content {
                        RegionContent::Midi { notes } => RegionSegs::scan(notes),
                        _ => RegionSegs::default(),
                    });
                let RegionContent::Midi { notes } = &r.content else {
                    s = rs.first.eval(s, |h| write_region_head(r, h));
                    continue;
                };
                let first_end = rs.runs.keys().next().copied();
                s = rs.first.eval(s, |h| {
                    write_region_head(r, h);
                    for n in run_notes(notes, None, first_end) {
                        write_note(n, h);
                    }
                });
                let mut runs = rs.runs.iter_mut().peekable();
                while let Some((from, seg)) = runs.next() {
                    let to = runs.peek().map(|(k, _)| **k);
                    let from = *from;
                    s = seg.eval(s, |h| {
                        for n in run_notes(notes, Some(from), to) {
                            write_note(n, h);
                        }
                    });
                }
            }
        }
        self.last = Some(s);
        s
    }
}
