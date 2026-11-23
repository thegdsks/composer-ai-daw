use crate::automation::{AutomationLane, AutomationPoint, AutomationTarget};
use crate::effects::{Effect, EffectChain, EffectOwner};
use crate::id::Id;
use crate::time::{Tick, TimeSignature};
use composer_theory::Key;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Tempo is stored as thousandths of a beat per minute so it stays an integer.
pub const DEFAULT_MBPM: u32 = 120_000;
pub const MIN_MBPM: u32 = 20_000;
pub const MAX_MBPM: u32 = 400_000;
/// Longest accepted name, in bytes.
pub const MAX_NAME_LEN: usize = 256;

/// Tempo and meter changes keyed by tick. Tick 0 always has an entry in each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoMap {
    /// Milli beats per minute at each tick.
    pub tempos: BTreeMap<Tick, u32>,
    pub meters: BTreeMap<Tick, TimeSignature>,
    /// Key changes (D23). Empty means no key is set; skipped on the wire when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<Tick, Key>,
}

impl Default for TempoMap {
    fn default() -> Self {
        Self {
            tempos: BTreeMap::from([(0, DEFAULT_MBPM)]),
            meters: BTreeMap::from([(0, TimeSignature::COMMON)]),
            keys: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Instrument,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Midi,
    Audio,
}

/// A note. `start` is relative to the start of its region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: Id,
    pub pitch: u8,
    pub velocity: u8,
    pub start: Tick,
    pub duration: Tick,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionContent {
    Midi {
        notes: BTreeMap<Id, Note>,
    },
    /// Audio clip placeholder with no source (older files).
    Audio,
    /// Audio clip that plays an asset (D40).
    Clip {
        /// SHA-256 of the audio file, 64 lowercase hex digits.
        asset: String,
        /// First source frame played.
        offset: u64,
        /// Clip gain in permille, 0..=MAX_VOLUME.
        gain: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub id: Id,
    pub start: Tick,
    pub length: Tick,
    /// Loop length in ticks (D24). 0 means no loop. The first `loop_length` ticks of the
    /// content repeat to fill `length`. Skipped in JSON and in the hash while 0.
    #[serde(default, skip_serializing_if = "is_zero_tick")]
    pub loop_length: Tick,
    pub content: RegionContent,
}

impl Region {
    pub fn kind(&self) -> RegionKind {
        match self.content {
            RegionContent::Midi { .. } => RegionKind::Midi,
            RegionContent::Audio | RegionContent::Clip { .. } => RegionKind::Audio,
        }
    }
}

/// Largest clip source offset in frames (D40), small enough to stay exact in JSON numbers.
pub const MAX_CLIP_OFFSET: u64 = 1 << 44;

/// Unity volume in permille.
pub const UNITY_VOLUME: u16 = 1000;
/// Loudest accepted volume in permille (about +6 dB).
pub const MAX_VOLUME: u16 = 2000;
/// Pan range in permille, -MAX_PAN full left to MAX_PAN full right.
pub const MAX_PAN: i16 = 1000;

/// Mixer state of one track. Integers only, so the model stays free of floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelMix {
    /// Permille of unity gain, 0..=MAX_VOLUME.
    pub volume: u16,
    /// Permille, -MAX_PAN..=MAX_PAN, 0 is centered.
    pub pan: i16,
    pub mute: bool,
    pub solo: bool,
}

impl Default for ChannelMix {
    fn default() -> Self {
        Self {
            volume: UNITY_VOLUME,
            pan: 0,
            mute: false,
            solo: false,
        }
    }
}

impl ChannelMix {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Largest master reverb size or mix, and largest per track send, in permille (D26).
pub const MAX_REVERB: u16 = 1000;

/// Master bus state (D26). Integers only. The default is unity volume with the reverb off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterMix {
    /// Permille of unity gain, 0..=MAX_VOLUME.
    pub volume: u16,
    /// Room size in permille, 0..=MAX_REVERB.
    pub reverb_size: u16,
    /// Reverb return level in permille, 0..=MAX_REVERB. Zero means the reverb is off.
    pub reverb_mix: u16,
}

impl Default for MasterMix {
    fn default() -> Self {
        Self {
            volume: UNITY_VOLUME,
            reverb_size: 500,
            reverb_mix: 0,
        }
    }
}

impl MasterMix {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn is_zero_tick(v: &Tick) -> bool {
    *v == 0
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: Id,
    pub name: String,
    pub kind: TrackKind,
    /// Opaque id of the instrument in the sound catalogue. Only for instrument tracks.
    pub instrument: Option<Id>,
    /// Skipped in JSON and in the state hash while it is the default, so older files stay valid.
    #[serde(default, skip_serializing_if = "ChannelMix::is_default")]
    pub mix: ChannelMix,
    /// Catalogue sound id as text (D29). Skipped in JSON and in the state hash while None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    /// Reverb send in permille, 0..=MAX_REVERB (D26). Skipped in JSON and in the hash while 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reverb_send: u16,
    pub regions: BTreeMap<Id, Region>,
}

/// Where an id lives, so lookups never scan the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Loc {
    Project,
    Track,
    Region { track: Id },
    Note { region: Id },
    Effect { owner: EffectOwner },
}

/// Derived lookup tables. Not part of the document: skipped by serde, equality
/// and `state_hash`, and rebuilt by [`Project::reindex`].
#[derive(Debug, Default)]
pub(crate) struct IdIndex {
    pub(crate) ids: HashMap<Id, Loc>,
    /// Position hint of each track in `Project::tracks`. A hint is trusted only
    /// after checking `tracks[hint].id`, so inserts and deletes need not touch
    /// the later tracks; a miss repairs the stale tail on demand.
    pub(crate) track_pos: HashMap<Id, AtomicUsize>,
    /// Hints at positions below this are known exact.
    pos_valid: AtomicUsize,
    /// Hash table probes made by lookups, so tests can assert cost without timing.
    probes: AtomicU64,
}

impl Clone for IdIndex {
    fn clone(&self) -> Self {
        Self {
            ids: self.ids.clone(),
            track_pos: self
                .track_pos
                .iter()
                .map(|(k, v)| (*k, AtomicUsize::new(v.load(Ordering::Relaxed))))
                .collect(),
            pos_valid: AtomicUsize::new(self.pos_valid.load(Ordering::Relaxed)),
            probes: AtomicU64::new(self.probes.load(Ordering::Relaxed)),
        }
    }
}

// Derived data never takes part in document equality.
impl PartialEq for IdIndex {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for IdIndex {}

impl IdIndex {
    fn loc(&self, id: Id) -> Option<Loc> {
        self.probes.fetch_add(1, Ordering::Relaxed);
        self.ids.get(&id).copied()
    }

    fn pos_hint(&self, id: Id) -> Option<usize> {
        self.probes.fetch_add(1, Ordering::Relaxed);
        Some(self.track_pos.get(&id)?.load(Ordering::Relaxed))
    }

    /// Rewrites the stale tail of the hints from the real track order.
    fn repair(&self, tracks: &[Track]) {
        let from = self.pos_valid.load(Ordering::Relaxed).min(tracks.len());
        for (i, t) in tracks.iter().enumerate().skip(from) {
            if let Some(h) = self.track_pos.get(&t.id) {
                h.store(i, Ordering::Relaxed);
            }
        }
        self.pos_valid.store(tracks.len(), Ordering::Relaxed);
    }

    /// Tracks at or after `at` moved, so their hints are no longer known exact.
    fn invalidate_from(&mut self, at: usize) {
        let v = self.pos_valid.get_mut();
        *v = (*v).min(at);
    }

    fn add_region(&mut self, track: Id, r: &Region) {
        self.ids.insert(r.id, Loc::Region { track });
        if let RegionContent::Midi { notes } = &r.content {
            for n in notes.keys() {
                self.ids.insert(*n, Loc::Note { region: r.id });
            }
        }
    }

    fn remove_region(&mut self, r: &Region) {
        self.ids.remove(&r.id);
        if let RegionContent::Midi { notes } = &r.content {
            for n in notes.keys() {
                self.ids.remove(n);
            }
        }
    }

    fn add_track(&mut self, t: &Track) {
        self.ids.insert(t.id, Loc::Track);
        for r in t.regions.values() {
            self.add_region(t.id, r);
        }
    }

    fn remove_track(&mut self, t: &Track) {
        self.ids.remove(&t.id);
        self.track_pos.remove(&t.id);
        for r in t.regions.values() {
            self.remove_region(r);
        }
    }
}

/// Wire shape of a project; the index is rebuilt after loading.
#[derive(Deserialize)]
struct ProjectData {
    id: Id,
    name: String,
    tempo_map: TempoMap,
    tracks: Vec<Track>,
    #[serde(default)]
    master: MasterMix,
    #[serde(default)]
    markers: BTreeMap<Tick, String>,
    #[serde(default)]
    automation: Vec<AutomationLane>,
    #[serde(default)]
    effects: Vec<EffectChain>,
}

impl From<ProjectData> for Project {
    fn from(d: ProjectData) -> Self {
        let mut p = Project {
            id: d.id,
            name: d.name,
            tempo_map: d.tempo_map,
            tracks: d.tracks,
            master: d.master,
            markers: d.markers,
            automation: d.automation,
            effects: d.effects,
            index: IdIndex::default(),
        };
        p.reindex();
        p
    }
}

/// The whole document. Mutate it only through [`crate::Change`] records
/// (`Project::apply`), so every edit stays invertible and the id index stays
/// in step. If `tracks` is ever edited directly, call [`Project::reindex`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ProjectData")]
pub struct Project {
    pub id: Id,
    pub name: String,
    pub tempo_map: TempoMap,
    /// Display order of tracks.
    pub tracks: Vec<Track>,
    /// Master volume and reverb (D26). Skipped in JSON and in the hash while default.
    #[serde(default, skip_serializing_if = "MasterMix::is_default")]
    pub master: MasterMix,
    /// Named positions on the timeline (D24), tick to name. Skipped while empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub markers: BTreeMap<Tick, String>,
    /// Automation lanes (D36), sorted by target, never empty. Skipped while there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automation: Vec<AutomationLane>,
    /// Effect chains (D45), sorted by owner, never empty. Skipped while there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectChain>,
    #[serde(skip)]
    pub(crate) index: IdIndex,
}

impl Project {
    pub fn new(id: Id, name: impl Into<String>) -> Self {
        let mut p = Self {
            id,
            name: name.into(),
            tempo_map: TempoMap::default(),
            tracks: Vec::new(),
            master: MasterMix::default(),
            markers: BTreeMap::new(),
            automation: Vec::new(),
            effects: Vec::new(),
            index: IdIndex::default(),
        };
        p.reindex();
        p
    }

    /// The lane driving `target`, if any.
    pub fn lane(&self, target: &AutomationTarget) -> Option<&AutomationLane> {
        let at = self
            .automation
            .binary_search_by(|l| l.target.cmp(target))
            .ok()?;
        self.automation.get(at)
    }

    /// Point of a lane at an exact tick.
    pub fn automation_point(
        &self,
        target: &AutomationTarget,
        tick: Tick,
    ) -> Option<AutomationPoint> {
        self.lane(target)?.points.get(&tick).copied()
    }

    /// Writes or removes one point, creating and dropping the lane as needed so lanes stay sorted
    /// and never empty. Range rules are checked by the caller.
    pub(crate) fn put_automation_point(
        &mut self,
        target: AutomationTarget,
        tick: Tick,
        point: Option<AutomationPoint>,
    ) {
        match self.automation.binary_search_by(|l| l.target.cmp(&target)) {
            Ok(at) => {
                let lane = &mut self.automation[at];
                match point {
                    Some(p) => {
                        lane.points.insert(tick, p);
                    }
                    None => {
                        lane.points.remove(&tick);
                    }
                }
                if lane.points.is_empty() {
                    self.automation.remove(at);
                }
            }
            Err(at) => {
                if let Some(p) = point {
                    self.automation.insert(
                        at,
                        AutomationLane {
                            target,
                            points: BTreeMap::from([(tick, p)]),
                        },
                    );
                }
            }
        }
    }

    /// The devices of one chain in processing order, empty when there is none.
    pub fn chain(&self, owner: &EffectOwner) -> &[Effect] {
        match self.effects.binary_search_by(|c| c.owner.cmp(owner)) {
            Ok(at) => &self.effects[at].effects,
            Err(_) => &[],
        }
    }

    /// A device by id with the owner of its chain and its position there.
    pub fn effect(&self, id: Id) -> Option<(EffectOwner, usize, &Effect)> {
        let Loc::Effect { owner } = self.index.loc(id)? else {
            return None;
        };
        let chain = self.chain(&owner);
        let at = chain.iter().position(|e| e.id == id)?;
        Some((owner, at, &chain[at]))
    }

    pub(crate) fn effect_mut(&mut self, id: Id) -> Option<&mut Effect> {
        let Loc::Effect { owner } = self.index.loc(id)? else {
            return None;
        };
        let at = self
            .effects
            .binary_search_by(|c| c.owner.cmp(&owner))
            .ok()?;
        self.effects[at].effects.iter_mut().find(|e| e.id == id)
    }

    /// Inserts a device at `index` (the caller checked the range), creating the chain.
    pub(crate) fn insert_effect(&mut self, owner: EffectOwner, index: usize, e: Effect) {
        self.index.ids.insert(e.id, Loc::Effect { owner });
        match self.effects.binary_search_by(|c| c.owner.cmp(&owner)) {
            Ok(at) => self.effects[at].effects.insert(index, e),
            Err(at) => self.effects.insert(
                at,
                EffectChain {
                    owner,
                    effects: vec![e],
                },
            ),
        }
    }

    /// Removes the device at `index` (the caller checked the range), dropping an emptied chain.
    pub(crate) fn remove_effect_at(&mut self, owner: EffectOwner, index: usize) -> Effect {
        let at = self
            .effects
            .binary_search_by(|c| c.owner.cmp(&owner))
            .expect("chain checked by the caller");
        let e = self.effects[at].effects.remove(index);
        if self.effects[at].effects.is_empty() {
            self.effects.remove(at);
        }
        self.index.ids.remove(&e.id);
        e
    }

    /// Rebuilds the id index from the document with one full scan.
    pub fn reindex(&mut self) {
        self.index = self.scan_index();
    }

    fn scan_index(&self) -> IdIndex {
        let mut ix = IdIndex::default();
        ix.ids.insert(self.id, Loc::Project);
        for (i, t) in self.tracks.iter().enumerate() {
            ix.add_track(t);
            ix.track_pos.insert(t.id, AtomicUsize::new(i));
        }
        for c in &self.effects {
            for e in &c.effects {
                ix.ids.insert(e.id, Loc::Effect { owner: c.owner });
            }
        }
        *ix.pos_valid.get_mut() = self.tracks.len();
        ix
    }

    /// True when the index equals a fresh full scan. For tests and debugging;
    /// it costs a scan.
    pub fn index_is_consistent(&self) -> bool {
        let fresh = self.scan_index();
        fresh.ids == self.index.ids
            && fresh.track_pos.len() == self.index.track_pos.len()
            && self
                .tracks
                .iter()
                .enumerate()
                .all(|(i, t)| self.track_index(t.id) == Some(i))
    }

    /// Hash table probes made by lookups so far (one per probe, never per element).
    pub fn index_probes(&self) -> u64 {
        self.index.probes.load(Ordering::Relaxed)
    }

    pub fn track(&self, id: Id) -> Option<&Track> {
        self.tracks.get(self.track_index(id)?)
    }

    pub fn track_index(&self, id: Id) -> Option<usize> {
        let hint = self.index.pos_hint(id)?;
        if self.tracks.get(hint).is_some_and(|t| t.id == id) {
            return Some(hint);
        }
        self.index.repair(&self.tracks);
        let pos = self.index.pos_hint(id)?;
        self.tracks
            .get(pos)
            .is_some_and(|t| t.id == id)
            .then_some(pos)
    }

    /// Finds a region and the track holding it.
    pub fn region(&self, id: Id) -> Option<(&Track, &Region)> {
        match self.index.loc(id)? {
            Loc::Region { track } => {
                let t = self.track(track)?;
                Some((t, t.regions.get(&id)?))
            }
            _ => None,
        }
    }

    pub fn note(&self, region: Id, note: Id) -> Option<&Note> {
        match &self.region(region)?.1.content {
            RegionContent::Midi { notes } => notes.get(&note),
            RegionContent::Audio | RegionContent::Clip { .. } => None,
        }
    }

    /// True when the project, a track, a region or a note already uses this id.
    pub fn contains_id(&self, id: Id) -> bool {
        self.index.loc(id).is_some()
    }
}

// Index upkeep for `Project::apply`, kept here next to the index itself.
impl Project {
    pub(crate) fn track_mut(&mut self, id: Id) -> Option<&mut Track> {
        let pos = self.track_index(id)?;
        self.tracks.get_mut(pos)
    }

    pub(crate) fn region_mut(&mut self, id: Id) -> Option<&mut Region> {
        let Loc::Region { track } = self.index.loc(id)? else {
            return None;
        };
        self.track_mut(track)?.regions.get_mut(&id)
    }

    pub(crate) fn insert_track(&mut self, at: usize, t: Track) {
        self.index.add_track(&t);
        self.index.track_pos.insert(t.id, AtomicUsize::new(at));
        // Later tracks shift by one; their hints are repaired when a lookup misses.
        self.index.invalidate_from(at);
        self.tracks.insert(at, t);
    }

    pub(crate) fn remove_track_at(&mut self, at: usize) -> Track {
        let t = self.tracks.remove(at);
        self.index.remove_track(&t);
        self.index.invalidate_from(at);
        t
    }

    pub(crate) fn index_region(&mut self, track: Id, r: &Region) {
        self.index.add_region(track, r);
    }

    pub(crate) fn unindex_region(&mut self, r: &Region) {
        self.index.remove_region(r);
    }

    pub(crate) fn index_note(&mut self, region: Id, note: Id) {
        self.index.ids.insert(note, Loc::Note { region });
    }

    pub(crate) fn unindex_note(&mut self, note: Id) {
        self.index.ids.remove(&note);
    }
}
