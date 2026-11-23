use crate::effects::EffectOwner;
use crate::id::Id;
use crate::project::{Note, Project, Region, RegionContent, Track, TrackKind};

pub(crate) const MULT: u64 = 0x0000_0100_0000_01B3;
pub(crate) const SEED: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64 over a canonical byte stream. Stable across runs and platforms.
pub(crate) struct Fnv {
    pub(crate) h: u64,
    /// Bytes written, used to size a segment table.
    pub(crate) n: u64,
    /// When set, bytes are collected here instead of folded (used to build segment tables).
    pub(crate) rec: Option<Vec<u8>>,
}

impl Fnv {
    pub(crate) fn new(h: u64) -> Self {
        Self { h, n: 0, rec: None }
    }
    fn bytes(&mut self, b: &[u8]) {
        self.n += b.len() as u64;
        if let Some(buf) = &mut self.rec {
            buf.extend_from_slice(b);
            return;
        }
        for &x in b {
            self.h ^= u64::from(x);
            self.h = self.h.wrapping_mul(MULT);
        }
    }
    fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }
    fn id(&mut self, v: Id) {
        self.bytes(&v.0.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.bytes(s.as_bytes());
    }
}

/// Everything before the tracks, including the track count.
pub(crate) fn write_header(p: &Project, h: &mut Fnv) {
    h.id(p.id);
    h.str(&p.name);
    h.u64(p.tempo_map.tempos.len() as u64);
    for (tick, mbpm) in &p.tempo_map.tempos {
        h.i64(*tick);
        h.u64(u64::from(*mbpm));
    }
    h.u64(p.tempo_map.meters.len() as u64);
    for (tick, m) in &p.tempo_map.meters {
        h.i64(*tick);
        h.u8(m.numerator);
        h.u8(m.denominator);
    }
    // Keys are hashed only when present so projects without a key keep their old hash.
    if !p.tempo_map.keys.is_empty() {
        h.u64(p.tempo_map.keys.len() as u64);
        for (tick, k) in &p.tempo_map.keys {
            h.i64(*tick);
            h.u8(k.root);
            h.u8(k.scale.index());
        }
    }
    // Master bus is hashed only when not default (D26).
    if !p.master.is_default() {
        h.u8(0xF2);
        h.u64(u64::from(p.master.volume));
        h.u64(u64::from(p.master.reverb_size));
        h.u64(u64::from(p.master.reverb_mix));
    }
    // Markers are hashed only when present (D24).
    if !p.markers.is_empty() {
        h.u8(0xF5);
        h.u64(p.markers.len() as u64);
        for (tick, name) in &p.markers {
            h.i64(*tick);
            h.str(name);
        }
    }
    // Automation is hashed only when present (D36).
    if !p.automation.is_empty() {
        h.u8(0xF6);
        h.u64(p.automation.len() as u64);
        for lane in &p.automation {
            h.u8(lane.target.code());
            if let Some(track) = lane.target.track() {
                h.id(track);
            }
            h.u64(lane.points.len() as u64);
            for (tick, pt) in &lane.points {
                h.i64(*tick);
                h.i64(i64::from(pt.value));
                h.u8(pt.curve.code());
            }
        }
    }
    // Effect chains are hashed only when present (D45).
    if !p.effects.is_empty() {
        h.u8(0xF7);
        h.u64(p.effects.len() as u64);
        for c in &p.effects {
            match c.owner {
                EffectOwner::Master => h.u8(0),
                EffectOwner::Track { track } => {
                    h.u8(1);
                    h.id(track);
                }
            }
            h.u64(c.effects.len() as u64);
            for e in &c.effects {
                h.id(e.id);
                h.u8(e.kind.code());
                h.u8(u8::from(e.bypass));
                h.u64(e.params.len() as u64);
                for v in &e.params {
                    h.i64(i64::from(*v));
                }
            }
        }
    }
    h.u64(p.tracks.len() as u64);
}

/// A track up to and including its region count.
pub(crate) fn write_track_head(t: &Track, h: &mut Fnv) {
    h.id(t.id);
    h.str(&t.name);
    h.u8(match t.kind {
        TrackKind::Instrument => 0,
        TrackKind::Audio => 1,
    });
    match t.instrument {
        Some(i) => {
            h.u8(1);
            h.id(i);
        }
        None => h.u8(0),
    }
    // Written only when not default, so hashes of older projects stay the same.
    if !t.mix.is_default() {
        h.u8(0xF0);
        h.u64(u64::from(t.mix.volume));
        h.u64(t.mix.pan as u64);
        h.u8(u8::from(t.mix.mute));
        h.u8(u8::from(t.mix.solo));
    }
    if let Some(sound) = &t.sound {
        h.u8(0xF1);
        h.str(sound);
    }
    if t.reverb_send != 0 {
        h.u8(0xF3);
        h.u64(u64::from(t.reverb_send));
    }
    h.u64(t.regions.len() as u64);
}

/// A region up to and including its note count (everything for non midi content).
pub(crate) fn write_region_head(r: &Region, h: &mut Fnv) {
    h.id(r.id);
    h.i64(r.start);
    h.i64(r.length);
    if r.loop_length != 0 {
        h.u8(0xF4);
        h.i64(r.loop_length);
    }
    match &r.content {
        RegionContent::Audio => h.u8(0),
        RegionContent::Clip {
            asset,
            offset,
            gain,
        } => {
            h.u8(2);
            h.str(asset);
            h.u64(*offset);
            h.u64(u64::from(*gain));
        }
        RegionContent::Midi { notes } => {
            h.u8(1);
            h.u64(notes.len() as u64);
        }
    }
}

pub(crate) fn write_note(n: &Note, h: &mut Fnv) {
    h.id(n.id);
    h.u8(n.pitch);
    h.u8(n.velocity);
    h.i64(n.start);
    h.i64(n.duration);
}

impl Project {
    /// Deterministic hash of the full document state, for drift detection.
    /// This is the from scratch reference; [`HashCache`] must always agree with it.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv::new(SEED);
        write_header(self, &mut h);
        for t in &self.tracks {
            write_track_head(t, &mut h);
            for r in t.regions.values() {
                write_region_head(r, &mut h);
                if let RegionContent::Midi { notes } = &r.content {
                    for n in notes.values() {
                        write_note(n, &mut h);
                    }
                }
            }
        }
        h.h
    }
}
