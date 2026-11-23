//! Binary blob codec, see the crate docs for the layout.

use composer_engine::{clamp_send, master_from_project, send_from_project};
use composer_engine::{
    compile_automation, compile_effects, compile_events, Adsr, AutoLane, AutoPoint, BuiltinVoice,
    ChainSpec, Event, EventKind, FxSpec, LaneParam, LoopMode, LoopSpan, MasterBus, SampleBank,
    SampleData, Zone, BUILTIN_SAW,
};
use composer_model::{
    AutomationCurve, ChannelMix, EffectKind, Id, Project, MAX_CHAIN_EFFECTS, MAX_EFFECT_PARAMS,
    MAX_PAN, MAX_VOLUME,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const MAGIC: u32 = 0x5750_4D43;
/// Written version when no sample bank is used. Version 1 (12 byte tracks, no mix) stays readable.
const VERSION: u32 = 2;
/// Version 2 plus a sample bank section after the events (D34).
const VERSION_3: u32 = 3;
/// Version 3 layout plus a master section between the events and the bank section (D26):
/// `f32 volume, f32 reverb_size, f32 reverb_mix, u32 zero`, then `track_count` x `f32 send`.
/// The bank section is always present in v4 (its count may be 0).
const VERSION_4: u32 = 4;
/// Version 4 layout plus a loop section after the bank section (D24): `u32 count`, then per
/// loop `u32 track, u64 start_frame, u64 period_frames, u64 end_frame`. The events stay
/// expanded, so a reader that ignores the section plays the same notes. Written only when a
/// clip has a loop that repeats, so every other project keeps its v2 to v4 bytes.
const VERSION_5: u32 = 5;
/// Version 5 layout (the loop section is always present, its count may be 0) plus an automation
/// section after it (D36): `u32 count`, then per lane a 16 byte header `u32 track (u32::MAX for
/// the master), u8 param, 3 zero bytes, f32 base, u32 points` followed by its points, each
/// `u64 frame, i16 value, u8 curve, 5 zero bytes`. Written only when the project has lanes, so
/// every other project keeps its v2 to v5 bytes.
const VERSION_6: u32 = 6;
/// Version 6 layout (the loop and automation sections are always present, their counts may be 0)
/// plus an effects section after them (D45): `u32 chain_count`, then per chain `u32 track (u32::MAX
/// for the master), u32 effect_count`, then per device a 20 byte record `u8 kind, u8 bypass,
/// u8 param_count, u8 zero, 8 x i16 params`. Written only when the project has a chain, so every
/// other project keeps its v2 to v6 bytes.
const VERSION_7: u32 = 7;
const CHAIN_HEAD: usize = 8;
const FX: usize = 20;
const LANE: usize = 16;
const POINT: usize = 16;
/// Most points one lane may carry in a blob.
const MAX_LANE_POINTS: usize = 1_000_000;
const LOOP: usize = 28;
const MASTER: usize = 16;
/// Bytes of one zone record in a bank section.
const ZONE: usize = 48;
const VERSION_1: u32 = 1;
const HEADER: usize = 32;
const TRACK_V1: usize = 12;
const TRACK: usize = 16;
const EVENT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackSpec {
    /// 0 sine, 1 saw, 2 to 17 the built in voices (`BuiltinVoice::wave_code`).
    pub waveform: u8,
    pub gain: f32,
    pub pan: f32,
    /// Project channel mix (D14). Solo is resolved across all tracks by the session.
    pub mix: ChannelMix,
    /// Index into [`Decoded::banks`] when the track plays a sample bank (blob v3), else a synth.
    pub bank: Option<u16>,
    /// Reverb send 0.0..=1.0 (blob v4, D26).
    pub send: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub sample_rate: u32,
    pub voices_per_track: u32,
    pub length_frames: u64,
    pub tracks: Vec<TrackSpec>,
    pub events: Vec<Event>,
    pub banks: Vec<Arc<SampleBank>>,
    /// Master volume and reverb (blob v4, D26).
    pub master: MasterBus,
    /// Looped clips (blob v5, D24).
    pub loops: Vec<LoopSpan>,
    /// Automation lanes (blob v6, D36).
    pub lanes: Vec<AutoLane>,
    /// Effect chains (blob v7, D45).
    pub effects: Vec<ChainSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobError {
    Truncated,
    Magic,
    Version,
    Waveform,
    EventKind,
    Rate,
    Mix,
    Bank,
    Master,
    Loop,
    Automation,
    Effects,
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[i..i + 8]);
    u64::from_le_bytes(a)
}

fn f32_at(b: &[u8], i: usize) -> f32 {
    f32::from_bits(u32_at(b, i))
}

pub fn decode(b: &[u8]) -> Result<Decoded, BlobError> {
    if b.len() < HEADER {
        return Err(BlobError::Truncated);
    }
    if u32_at(b, 0) != MAGIC {
        return Err(BlobError::Magic);
    }
    let version = u32_at(b, 4);
    if version != VERSION
        && version != VERSION_1
        && version != VERSION_3
        && version != VERSION_4
        && version != VERSION_5
        && version != VERSION_6
        && version != VERSION_7
    {
        return Err(BlobError::Version);
    }
    let track_size = if version == VERSION_1 {
        TRACK_V1
    } else {
        TRACK
    };
    let sample_rate = u32_at(b, 8);
    if sample_rate == 0 {
        return Err(BlobError::Rate);
    }
    let voices_per_track = u32_at(b, 12);
    let nt = u32_at(b, 16) as usize;
    let ne = u32_at(b, 20) as usize;
    let length_frames = u64_at(b, 24);
    let need = nt
        .checked_mul(track_size)
        .and_then(|t| ne.checked_mul(EVENT).and_then(|e| t.checked_add(e)))
        .and_then(|x| x.checked_add(HEADER))
        .ok_or(BlobError::Truncated)?;
    if b.len() < need {
        return Err(BlobError::Truncated);
    }
    let mut tracks: Vec<TrackSpec> = Vec::with_capacity(nt);
    for i in 0..nt {
        let o = HEADER + i * track_size;
        if b[o] > 1 && BuiltinVoice::from_wave_code(b[o]).is_none() {
            return Err(BlobError::Waveform);
        }
        let mix = if version != VERSION_1 {
            let volume = u16::from_le_bytes([b[o + 2], b[o + 3]]);
            let pan = i16::from_le_bytes([b[o + 12], b[o + 13]]);
            if volume > MAX_VOLUME || !(-MAX_PAN..=MAX_PAN).contains(&pan) {
                return Err(BlobError::Mix);
            }
            ChannelMix {
                volume,
                pan,
                mute: b[o + 1] & 1 != 0,
                solo: b[o + 1] & 2 != 0,
            }
        } else {
            ChannelMix::default()
        };
        let bank = if version >= VERSION_3 {
            u16::from_le_bytes([b[o + 14], b[o + 15]]).checked_sub(1)
        } else {
            None
        };
        tracks.push(TrackSpec {
            waveform: b[o],
            gain: f32_at(b, o + 4),
            pan: f32_at(b, o + 8),
            mix,
            bank,
            send: 0.0,
        });
    }
    let mut events = Vec::with_capacity(ne);
    for i in 0..ne {
        let o = HEADER + nt * track_size + i * EVENT;
        let kind = match b[o + 12] {
            0 => EventKind::NoteOff { pitch: b[o + 13] },
            1 => EventKind::NoteOn {
                pitch: b[o + 13],
                velocity: b[o + 14],
            },
            _ => return Err(BlobError::EventKind),
        };
        events.push(Event {
            frame: u64_at(b, o),
            track: u32_at(b, o + 8),
            kind,
        });
    }
    let mut master = MasterBus::default();
    let mut rest = need;
    if version >= VERSION_4 {
        let sends_end = nt
            .checked_mul(4)
            .and_then(|x| x.checked_add(MASTER))
            .and_then(|x| x.checked_add(need))
            .ok_or(BlobError::Truncated)?;
        if b.len() < sends_end {
            return Err(BlobError::Truncated);
        }
        master = MasterBus {
            volume: f32_at(b, need),
            reverb_size: f32_at(b, need + 4),
            reverb_mix: f32_at(b, need + 8),
        };
        if !master.is_valid() || u32_at(b, need + 12) != 0 {
            return Err(BlobError::Master);
        }
        for (i, t) in tracks.iter_mut().enumerate() {
            let send = f32_at(b, need + MASTER + i * 4);
            if !send.is_finite() || !(0.0..=1.0).contains(&send) {
                return Err(BlobError::Master);
            }
            t.send = send;
        }
        rest = sends_end;
    }
    let mut loops = Vec::new();
    let mut lanes = Vec::new();
    let mut effects = Vec::new();
    let banks = if version >= VERSION_3 {
        let (banks, used) = decode_banks(&b[rest..])?;
        if version >= VERSION_5 {
            let (l, loop_bytes) = decode_loops(&b[rest + used..], nt)?;
            loops = l;
            if version >= VERSION_6 {
                let (l, lane_bytes) = decode_lanes(&b[rest + used + loop_bytes..], nt)?;
                lanes = l;
                if version >= VERSION_7 {
                    effects = decode_effects(&b[rest + used + loop_bytes + lane_bytes..], nt)?;
                }
            }
        }
        banks
    } else {
        Vec::new()
    };
    if tracks
        .iter()
        .any(|t| t.bank.is_some_and(|i| usize::from(i) >= banks.len()))
    {
        return Err(BlobError::Bank);
    }
    Ok(Decoded {
        sample_rate,
        voices_per_track,
        length_frames,
        tracks,
        events,
        banks,
        master,
        loops,
        lanes,
        effects,
    })
}

/// Reads the bank section. Every count is checked against the bytes left before anything is
/// allocated, so a hostile header cannot ask for more memory than the blob itself holds.
fn decode_banks(b: &[u8]) -> Result<(Vec<Arc<SampleBank>>, usize), BlobError> {
    let mut at = 0usize;
    let mut take = |n: usize| -> Result<&[u8], BlobError> {
        let end = at.checked_add(n).ok_or(BlobError::Truncated)?;
        let s = b.get(at..end).ok_or(BlobError::Truncated)?;
        at = end;
        Ok(s)
    };
    let count = u32_at(take(4)?, 0) as usize;
    // A bank is at least 8 bytes, so the count is bounded by what is left.
    if count > b.len() / 8 || count >= usize::from(u16::MAX) {
        return Err(BlobError::Bank);
    }
    let mut banks = Vec::with_capacity(count);
    for _ in 0..count {
        let head = take(8)?;
        let (ns, nz) = (u32_at(head, 0) as usize, u32_at(head, 4) as usize);
        if ns > b.len() / 12 || nz > b.len() / ZONE {
            return Err(BlobError::Bank);
        }
        let mut samples = Vec::with_capacity(ns);
        for _ in 0..ns {
            let h = take(12)?;
            let (rate, channels, frames) = (u32_at(h, 0), u32_at(h, 4), u32_at(h, 8) as usize);
            if channels == 0 || channels > 2 {
                return Err(BlobError::Bank);
            }
            let bytes = frames
                .checked_mul(channels as usize)
                .and_then(|x| x.checked_mul(4))
                .ok_or(BlobError::Bank)?;
            let raw = take(bytes)?;
            let data: Vec<f32> = raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_bits(u32::from_le_bytes(*c)))
                .collect();
            if data.iter().any(|x| !x.is_finite()) {
                return Err(BlobError::Bank);
            }
            samples
                .push(SampleData::new(rate, channels as u8, 0, data).map_err(|_| BlobError::Bank)?);
        }
        let mut zones = Vec::with_capacity(nz);
        for _ in 0..nz {
            let z = take(ZONE)?;
            let loop_mode = match z[9] {
                0 => LoopMode::None,
                1 => LoopMode::Continuous,
                2 => LoopMode::Sustain,
                3 => LoopMode::OneShot,
                _ => return Err(BlobError::Bank),
            };
            zones.push(Zone {
                sample: u32_at(z, 0),
                key_lo: z[4],
                key_hi: z[5],
                vel_lo: z[6],
                vel_hi: z[7],
                root_key: z[8],
                loop_mode,
                scale_cents: i16::from_le_bytes([z[10], z[11]]),
                tune_cents: u32_at(z, 12) as i32,
                loop_start: u32_at(z, 16),
                loop_end: u32_at(z, 20),
                adsr: Adsr {
                    attack: f32_at(z, 24),
                    decay: f32_at(z, 28),
                    sustain: f32_at(z, 32),
                    release: f32_at(z, 36),
                },
                gain: f32_at(z, 40),
                pan: f32_at(z, 44),
            });
        }
        banks.push(Arc::new(
            SampleBank::new(samples, zones).map_err(|_| BlobError::Bank)?,
        ));
    }
    Ok((banks, at))
}

/// Reads the loop section. The count is checked against the bytes left before allocating.
fn decode_loops(b: &[u8], tracks: usize) -> Result<(Vec<LoopSpan>, usize), BlobError> {
    let head = b.get(..4).ok_or(BlobError::Truncated)?;
    let count = u32_at(head, 0) as usize;
    if count > (b.len() - 4) / LOOP {
        return Err(BlobError::Truncated);
    }
    let mut loops = Vec::with_capacity(count);
    for i in 0..count {
        let o = 4 + i * LOOP;
        let l = LoopSpan {
            track: u32_at(b, o),
            start_frame: u64_at(b, o + 4),
            period_frames: u64_at(b, o + 12),
            end_frame: u64_at(b, o + 20),
        };
        if l.track as usize >= tracks || l.period_frames == 0 || l.end_frame < l.start_frame {
            return Err(BlobError::Loop);
        }
        loops.push(l);
    }
    Ok((loops, 4 + count * LOOP))
}

/// Reads the automation section. Counts are checked against the bytes left before allocating.
fn decode_lanes(b: &[u8], tracks: usize) -> Result<(Vec<AutoLane>, usize), BlobError> {
    let head = b.get(..4).ok_or(BlobError::Truncated)?;
    let count = u32_at(head, 0) as usize;
    if count > (b.len() - 4) / LANE {
        return Err(BlobError::Truncated);
    }
    let mut at = 4usize;
    let mut lanes = Vec::with_capacity(count);
    for _ in 0..count {
        let h = b.get(at..at + LANE).ok_or(BlobError::Truncated)?;
        let track = match u32_at(h, 0) {
            u32::MAX => None,
            t if (t as usize) < tracks => Some(t),
            _ => return Err(BlobError::Automation),
        };
        let param = LaneParam::from_code(h[4]).ok_or(BlobError::Automation)?;
        let base = f32_at(h, 8);
        let np = u32_at(h, 12) as usize;
        let valid_target = (track.is_none()) == (param == LaneParam::MasterVolume);
        if !valid_target || np == 0 || np > MAX_LANE_POINTS || !base.is_finite() {
            return Err(BlobError::Automation);
        }
        at += LANE;
        let bytes = np.checked_mul(POINT).ok_or(BlobError::Truncated)?;
        let raw = at
            .checked_add(bytes)
            .and_then(|end| b.get(at..end))
            .ok_or(BlobError::Truncated)?;
        let mut points = Vec::with_capacity(np);
        for c in raw.as_chunks::<POINT>().0 {
            let curve = match c[10] {
                0 => AutomationCurve::Hold,
                1 => AutomationCurve::Linear,
                2 => AutomationCurve::Smooth,
                _ => return Err(BlobError::Automation),
            };
            let p = AutoPoint {
                frame: u64_at(c, 0),
                value: i16::from_le_bytes([c[8], c[9]]),
                curve,
            };
            if points.last().is_some_and(|q: &AutoPoint| q.frame > p.frame) {
                return Err(BlobError::Automation);
            }
            points.push(p);
        }
        at += bytes;
        lanes.push(AutoLane {
            track,
            param,
            points,
            base,
        });
    }
    Ok((lanes, at))
}

/// Reads the effects section. Counts are checked against the bytes left before allocating, and
/// every parameter must lie in the range of its kind.
fn decode_effects(b: &[u8], tracks: usize) -> Result<Vec<ChainSpec>, BlobError> {
    let head = b.get(..4).ok_or(BlobError::Truncated)?;
    let count = u32_at(head, 0) as usize;
    if count > (b.len() - 4) / CHAIN_HEAD {
        return Err(BlobError::Truncated);
    }
    let mut at = 4usize;
    let mut chains: Vec<ChainSpec> = Vec::with_capacity(count);
    for _ in 0..count {
        let h = b.get(at..at + CHAIN_HEAD).ok_or(BlobError::Truncated)?;
        let track = match u32_at(h, 0) {
            u32::MAX => None,
            t if (t as usize) < tracks => Some(t),
            _ => return Err(BlobError::Effects),
        };
        let n = u32_at(h, 4) as usize;
        if n == 0 || n > MAX_CHAIN_EFFECTS || chains.iter().any(|c| c.track == track) {
            return Err(BlobError::Effects);
        }
        at += CHAIN_HEAD;
        let raw = n
            .checked_mul(FX)
            .and_then(|bytes| at.checked_add(bytes))
            .and_then(|end| b.get(at..end))
            .ok_or(BlobError::Truncated)?;
        let mut effects = Vec::with_capacity(n);
        for c in raw.as_chunks::<FX>().0 {
            let kind = EffectKind::from_code(c[0]).ok_or(BlobError::Effects)?;
            let specs = kind.params();
            if c[1] > 1 || usize::from(c[2]) != specs.len() || c[3] != 0 {
                return Err(BlobError::Effects);
            }
            let mut spec = FxSpec::new(kind);
            spec.bypass = c[1] == 1;
            for (i, s) in specs.iter().enumerate() {
                let v = i16::from_le_bytes([c[4 + i * 2], c[5 + i * 2]]);
                if !(s.min..=s.max).contains(&v) {
                    return Err(BlobError::Effects);
                }
                spec.params[i] = v;
            }
            if c[4 + specs.len() * 2..].iter().any(|x| *x != 0) {
                return Err(BlobError::Effects);
            }
            effects.push(spec);
        }
        at += n * FX;
        chains.push(ChainSpec { track, effects });
    }
    Ok(chains)
}

fn encode_banks(b: &mut Vec<u8>, banks: &[Arc<SampleBank>]) {
    b.extend_from_slice(&(banks.len() as u32).to_le_bytes());
    for bank in banks {
        b.extend_from_slice(&(bank.samples().len() as u32).to_le_bytes());
        b.extend_from_slice(&(bank.zones().len() as u32).to_le_bytes());
        for s in bank.samples() {
            b.extend_from_slice(&s.rate().to_le_bytes());
            b.extend_from_slice(&u32::from(s.channels()).to_le_bytes());
            b.extend_from_slice(&(s.len_frames() as u32).to_le_bytes());
            for x in s.data() {
                b.extend_from_slice(&x.to_bits().to_le_bytes());
            }
        }
        for z in bank.zones() {
            b.extend_from_slice(&z.sample.to_le_bytes());
            let mode = match z.loop_mode {
                LoopMode::None => 0,
                LoopMode::Continuous => 1,
                LoopMode::Sustain => 2,
                LoopMode::OneShot => 3,
            };
            b.extend_from_slice(&[z.key_lo, z.key_hi, z.vel_lo, z.vel_hi, z.root_key, mode]);
            b.extend_from_slice(&z.scale_cents.to_le_bytes());
            b.extend_from_slice(&z.tune_cents.to_le_bytes());
            b.extend_from_slice(&z.loop_start.to_le_bytes());
            b.extend_from_slice(&z.loop_end.to_le_bytes());
            for f in [
                z.adsr.attack,
                z.adsr.decay,
                z.adsr.sustain,
                z.adsr.release,
                z.gain,
                z.pan,
            ] {
                b.extend_from_slice(&f.to_bits().to_le_bytes());
            }
        }
    }
}

pub fn encode(d: &Decoded) -> Vec<u8> {
    let mut b = Vec::with_capacity(HEADER + d.tracks.len() * TRACK + d.events.len() * EVENT);
    b.extend_from_slice(&MAGIC.to_le_bytes());
    let bus = d.master != MasterBus::default() || d.tracks.iter().any(|t| t.send != 0.0);
    let bus = bus || !d.loops.is_empty();
    let bus = bus || !d.lanes.is_empty() || !d.effects.is_empty();
    let version = if !d.effects.is_empty() {
        VERSION_7
    } else if !d.lanes.is_empty() {
        VERSION_6
    } else if !d.loops.is_empty() {
        VERSION_5
    } else if bus {
        VERSION_4
    } else if d.banks.is_empty() {
        VERSION
    } else {
        VERSION_3
    };
    b.extend_from_slice(&version.to_le_bytes());
    b.extend_from_slice(&d.sample_rate.to_le_bytes());
    b.extend_from_slice(&d.voices_per_track.to_le_bytes());
    b.extend_from_slice(&(d.tracks.len() as u32).to_le_bytes());
    b.extend_from_slice(&(d.events.len() as u32).to_le_bytes());
    b.extend_from_slice(&d.length_frames.to_le_bytes());
    for t in &d.tracks {
        let flags = u8::from(t.mix.mute) | (u8::from(t.mix.solo) << 1);
        b.extend_from_slice(&[t.waveform, flags]);
        b.extend_from_slice(&t.mix.volume.to_le_bytes());
        b.extend_from_slice(&t.gain.to_bits().to_le_bytes());
        b.extend_from_slice(&t.pan.to_bits().to_le_bytes());
        b.extend_from_slice(&t.mix.pan.to_le_bytes());
        let bank = if d.banks.is_empty() {
            0
        } else {
            t.bank.map_or(0, |i| i + 1)
        };
        b.extend_from_slice(&bank.to_le_bytes());
    }
    for e in &d.events {
        b.extend_from_slice(&e.frame.to_le_bytes());
        b.extend_from_slice(&e.track.to_le_bytes());
        let (k, p, v) = match e.kind {
            EventKind::NoteOff { pitch } => (0, pitch, 0),
            EventKind::NoteOn { pitch, velocity } => (1, pitch, velocity),
        };
        b.extend_from_slice(&[k, p, v, 0]);
    }
    if bus {
        for f in [d.master.volume, d.master.reverb_size, d.master.reverb_mix] {
            b.extend_from_slice(&f.to_bits().to_le_bytes());
        }
        b.extend_from_slice(&0u32.to_le_bytes());
        for t in &d.tracks {
            b.extend_from_slice(&t.send.to_bits().to_le_bytes());
        }
    }
    if bus || !d.banks.is_empty() {
        encode_banks(&mut b, &d.banks);
    }
    if !d.loops.is_empty() || !d.lanes.is_empty() || !d.effects.is_empty() {
        b.extend_from_slice(&(d.loops.len() as u32).to_le_bytes());
        for l in &d.loops {
            b.extend_from_slice(&l.track.to_le_bytes());
            b.extend_from_slice(&l.start_frame.to_le_bytes());
            b.extend_from_slice(&l.period_frames.to_le_bytes());
            b.extend_from_slice(&l.end_frame.to_le_bytes());
        }
    }
    if !d.lanes.is_empty() || !d.effects.is_empty() {
        b.extend_from_slice(&(d.lanes.len() as u32).to_le_bytes());
        for l in &d.lanes {
            b.extend_from_slice(&l.track.unwrap_or(u32::MAX).to_le_bytes());
            b.extend_from_slice(&[l.param.code(), 0, 0, 0]);
            b.extend_from_slice(&l.base.to_bits().to_le_bytes());
            b.extend_from_slice(&(l.points.len() as u32).to_le_bytes());
            for p in &l.points {
                b.extend_from_slice(&p.frame.to_le_bytes());
                b.extend_from_slice(&p.value.to_le_bytes());
                b.extend_from_slice(&[p.curve.code(), 0, 0, 0, 0, 0]);
            }
        }
    }
    if !d.effects.is_empty() {
        b.extend_from_slice(&(d.effects.len() as u32).to_le_bytes());
        for c in &d.effects {
            b.extend_from_slice(&c.track.unwrap_or(u32::MAX).to_le_bytes());
            b.extend_from_slice(&(c.effects.len() as u32).to_le_bytes());
            for e in &c.effects {
                let n = e.kind.params().len();
                b.extend_from_slice(&[e.kind.code(), u8::from(e.bypass), n as u8, 0]);
                for v in &e.params[..MAX_EFFECT_PARAMS] {
                    b.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
    }
    b
}

/// Compiles a project to blob v2 (v1 plus the channel mix of each track): one track spec per project track (saw when the track uses
/// the built-in saw, else sine), `tail_seconds` of release after the last note.
pub fn compile_project(p: &Project, sample_rate: u32, tail_seconds: f64) -> Vec<u8> {
    compile_project_with_banks(p, sample_rate, tail_seconds, &BTreeMap::new())
}

/// Stable stand in instrument id for a catalogue sound id (two FNV-1a lanes), so a sampled sound
/// can ride the existing bank path keyed by `Track.instrument` (D34 join point, D37).
pub fn sound_bank_key(sound: &str) -> Id {
    let mut lanes = [0xcbf2_9ce4_8422_2325u64, 0x8422_2325_cbf2_9ce4u64];
    for (i, lane) in lanes.iter_mut().enumerate() {
        for b in sound.bytes().chain(std::iter::once(i as u8)) {
            *lane ^= u64::from(b);
            *lane = lane.wrapping_mul(0x0100_0000_01b3);
        }
    }
    Id((u128::from(lanes[0]) << 64) | u128::from(lanes[1]))
}

/// Like [`compile_project`], and a track whose `sound` is a key of `sounds` plays that sample bank
/// (blob v3). A project that uses none of them compiles to the same bytes as [`compile_project`].
pub fn compile_project_with_sounds(
    p: &Project,
    sample_rate: u32,
    tail_seconds: f64,
    sounds: &BTreeMap<String, Arc<SampleBank>>,
) -> Vec<u8> {
    let mut banks: BTreeMap<Id, Arc<SampleBank>> = BTreeMap::new();
    let mut q = p.clone();
    for t in &mut q.tracks {
        if let Some(bank) = t.sound.as_deref().and_then(|s| sounds.get(s)) {
            let key = sound_bank_key(t.sound.as_deref().unwrap_or_default());
            banks.insert(key, Arc::clone(bank));
            t.instrument = Some(key);
        }
    }
    if banks.is_empty() {
        return compile_project(p, sample_rate, tail_seconds);
    }
    compile_project_with_banks(&q, sample_rate, tail_seconds, &banks)
}

/// Like [`compile_project`] (master and sends come from the project, D26), and a track whose instrument id is a key of `banks` plays that
/// bank (blob v3, D34). The blob is v2 when no track uses a bank.
pub fn compile_project_with_banks(
    p: &Project,
    sample_rate: u32,
    tail_seconds: f64,
    banks: &BTreeMap<Id, Arc<SampleBank>>,
) -> Vec<u8> {
    let sends: BTreeMap<Id, f32> = p
        .tracks
        .iter()
        .filter(|t| t.reverb_send != 0)
        .map(|t| (t.id, send_from_project(t)))
        .collect();
    compile_project_with_bus(
        p,
        sample_rate,
        tail_seconds,
        banks,
        master_from_project(p),
        &sends,
    )
}

/// Like [`compile_project_with_banks`] with the master bus and a reverb send per track id (D26).
/// The blob is v4 only when the master is not the default or a send is non zero, so a project
/// without bus settings keeps its v2 or v3 bytes.
pub fn compile_project_with_bus(
    p: &Project,
    sample_rate: u32,
    tail_seconds: f64,
    banks: &BTreeMap<Id, Arc<SampleBank>>,
    master: MasterBus,
    sends: &BTreeMap<Id, f32>,
) -> Vec<u8> {
    let schedule = compile_events(p, sample_rate);
    let tail = (tail_seconds * f64::from(sample_rate)).round() as u64;
    let mut used: Vec<Id> = Vec::new();
    let tracks = p
        .tracks
        .iter()
        .map(|t| {
            let bank = t.instrument.filter(|id| banks.contains_key(id)).map(|id| {
                match used.iter().position(|u| *u == id) {
                    Some(at) => at as u16,
                    None => {
                        used.push(id);
                        (used.len() - 1) as u16
                    }
                }
            });
            TrackSpec {
                waveform: t
                    .sound
                    .as_deref()
                    .and_then(BuiltinVoice::from_sound_id)
                    .map_or(
                        u8::from(t.instrument == Some(BUILTIN_SAW)),
                        BuiltinVoice::wave_code,
                    ),
                gain: 1.0,
                pan: 0.0,
                mix: t.mix,
                bank,
                send: sends.get(&t.id).copied().map_or(0.0, clamp_send),
            }
        })
        .collect();
    encode(&Decoded {
        sample_rate,
        voices_per_track: 16,
        length_frames: schedule.end_frame + tail,
        tracks,
        events: schedule.events,
        banks: used.iter().map(|id| Arc::clone(&banks[id])).collect(),
        master: MasterBus::new(master.volume, master.reverb_size, master.reverb_mix),
        loops: schedule.loops,
        lanes: compile_automation(p, sample_rate),
        effects: compile_effects(p),
    })
}
