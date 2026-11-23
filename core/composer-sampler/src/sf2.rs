//! SoundFont 2 reader. Facts: notes/ref-sampler-formats.yaml, section sf2.

use crate::{u16_at, u32_at, Chunks, Limits, SamplerError};
use composer_engine::{Adsr, LoopMode, SampleBank, SampleData, Zone};

const PHDR: usize = 38;
const BAG: usize = 4;
const GEN: usize = 4;
const INST: usize = 22;
const SHDR: usize = 46;

// Generator operators (SF2 2.04 section 8.1.2) that this reader uses.
const G_START_LOOP_FINE: usize = 2;
const G_END_LOOP_FINE: usize = 3;
const G_PAN: usize = 17;
const G_ATTACK: usize = 34;
const G_DECAY: usize = 36;
const G_SUSTAIN: usize = 37;
const G_RELEASE: usize = 38;
const G_INSTRUMENT: usize = 41;
const G_KEY_RANGE: usize = 43;
const G_VEL_RANGE: usize = 44;
const G_START_LOOP_COARSE: usize = 45;
const G_ATTENUATION: usize = 48;
const G_END_LOOP_COARSE: usize = 50;
const G_COARSE_TUNE: usize = 51;
const G_FINE_TUNE: usize = 52;
const G_SAMPLE_ID: usize = 53;
const G_SAMPLE_MODES: usize = 54;
const G_SCALE_TUNING: usize = 56;
const G_ROOT_KEY: usize = 58;

/// Which preset to read. `None` takes the first preset in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sf2Select {
    pub bank: u16,
    pub preset: u16,
}

#[derive(Clone, Copy)]
struct Gen {
    op: u16,
    amount: u16,
}

/// Generator values of one zone, indexed by operator (only operators below 64 are kept).
#[derive(Clone, Copy)]
struct GenSet {
    set: u64,
    amount: [u16; 64],
}

impl Default for GenSet {
    fn default() -> Self {
        Self {
            set: 0,
            amount: [0; 64],
        }
    }
}

impl GenSet {
    fn of(gens: &[Gen]) -> Self {
        let mut s = Self::default();
        for g in gens {
            if g.op < 64 {
                s.set |= 1 << g.op;
                s.amount[usize::from(g.op)] = g.amount;
            }
        }
        s
    }

    /// Values of `zone` win over `self` (the global zone).
    fn over(mut self, zone: &GenSet) -> Self {
        for op in 0..64 {
            if zone.set & (1 << op) != 0 {
                self.set |= 1 << op;
                self.amount[op] = zone.amount[op];
            }
        }
        self
    }

    fn int(&self, op: usize) -> Option<i32> {
        (self.set & (1 << op) != 0).then(|| i32::from(self.amount[op] as i16))
    }

    fn range(&self, op: usize) -> (u8, u8) {
        if self.set & (1 << op) != 0 {
            let a = self.amount[op];
            (a as u8, (a >> 8) as u8)
        } else {
            (0, 127)
        }
    }
}

struct Shdr {
    start: u32,
    end: u32,
    loop_start: u32,
    loop_end: u32,
    rate: u32,
    root: u8,
    correction: i8,
    kind: u16,
}

struct Tables<'a> {
    phdr: Vec<(u16, u16, u16)>,
    pbag: Vec<u16>,
    pgen: Vec<Gen>,
    inst: Vec<u16>,
    ibag: Vec<u16>,
    igen: Vec<Gen>,
    shdr: Vec<Shdr>,
    smpl: &'a [u8],
}

fn records(body: &[u8], size: usize) -> Result<std::slice::ChunksExact<'_, u8>, SamplerError> {
    if body.is_empty() || !body.len().is_multiple_of(size) {
        return Err(SamplerError::Malformed);
    }
    Ok(body.chunks_exact(size))
}

fn gens(body: &[u8]) -> Result<Vec<Gen>, SamplerError> {
    records(body, GEN)?
        .map(|r| {
            Ok(Gen {
                op: u16_at(r, 0)?,
                amount: u16_at(r, 2)?,
            })
        })
        .collect()
}

fn bag_gens(body: &[u8]) -> Result<Vec<u16>, SamplerError> {
    records(body, BAG)?.map(|r| u16_at(r, 0)).collect()
}

fn parse<'a>(bytes: &'a [u8]) -> Result<Tables<'a>, SamplerError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"sfbk" {
        return Err(SamplerError::Malformed);
    }
    let mut smpl: Option<&[u8]> = None;
    let mut found: [Option<&[u8]>; 7] = [None; 7];
    for top in Chunks::new(&bytes[12..]) {
        let (id, body) = top?;
        if &id != b"LIST" || body.len() < 4 {
            continue;
        }
        let kind = &body[0..4];
        for sub in Chunks::new(&body[4..]) {
            let (sid, sbody) = sub?;
            match (kind, &sid) {
                (b"sdta", b"smpl") => smpl = Some(sbody),
                (b"pdta", b"phdr") => found[0] = Some(sbody),
                (b"pdta", b"pbag") => found[1] = Some(sbody),
                (b"pdta", b"pgen") => found[2] = Some(sbody),
                (b"pdta", b"inst") => found[3] = Some(sbody),
                (b"pdta", b"ibag") => found[4] = Some(sbody),
                (b"pdta", b"igen") => found[5] = Some(sbody),
                (b"pdta", b"shdr") => found[6] = Some(sbody),
                _ => {}
            }
        }
    }
    let need = |i: usize| found[i].ok_or(SamplerError::Malformed);
    let phdr = records(need(0)?, PHDR)?
        .map(|r| Ok((u16_at(r, 20)?, u16_at(r, 22)?, u16_at(r, 24)?)))
        .collect::<Result<Vec<_>, SamplerError>>()?;
    let inst = records(need(3)?, INST)?
        .map(|r| u16_at(r, 20))
        .collect::<Result<Vec<_>, _>>()?;
    let shdr = records(need(6)?, SHDR)?
        .map(|r| {
            Ok(Shdr {
                start: u32_at(r, 20)?,
                end: u32_at(r, 24)?,
                loop_start: u32_at(r, 28)?,
                loop_end: u32_at(r, 32)?,
                rate: u32_at(r, 36)?,
                root: r[40],
                correction: r[41] as i8,
                kind: u16_at(r, 44)?,
            })
        })
        .collect::<Result<Vec<_>, SamplerError>>()?;
    Ok(Tables {
        phdr,
        pbag: bag_gens(need(1)?)?,
        pgen: gens(need(2)?)?,
        inst,
        ibag: bag_gens(need(4)?)?,
        igen: gens(need(5)?)?,
        shdr,
        smpl: smpl.ok_or(SamplerError::Malformed)?,
    })
}

/// Generators of bag `b` and the zone link (instrument or sample id) if it has one.
fn zone_of(
    bags: &[u16],
    gens: &[Gen],
    b: usize,
    link_op: usize,
) -> Result<(GenSet, Option<u16>), SamplerError> {
    let (Some(&lo), Some(&hi)) = (bags.get(b), bags.get(b + 1)) else {
        return Err(SamplerError::BadIndex);
    };
    let list = gens
        .get(usize::from(lo)..usize::from(hi))
        .ok_or(SamplerError::BadIndex)?;
    match list.split_last() {
        Some((last, rest)) if usize::from(last.op) == link_op => {
            Ok((GenSet::of(rest), Some(last.amount)))
        }
        _ => Ok((GenSet::of(list), None)),
    }
}

fn pow10(exp: f64) -> f64 {
    10f64.powf(exp)
}

fn intersect(a: (u8, u8), b: (u8, u8)) -> Option<(u8, u8)> {
    let lo = a.0.max(b.0);
    let hi = a.1.min(b.1).min(127);
    (lo <= hi).then_some((lo, hi))
}

/// Reads one preset of a SoundFont 2 file into a bank.
///
/// Preset and instrument zones are flattened (preset values add to instrument values, ranges
/// intersect). Envelope shapes are approximated with the engine's linear stages.
pub fn load_sf2(
    bytes: &[u8],
    limits: &Limits,
    select: Option<Sf2Select>,
) -> Result<SampleBank, SamplerError> {
    if bytes.len() > limits.max_file_bytes {
        return Err(SamplerError::TooLarge);
    }
    let t = parse(bytes)?;
    let smpl_frames = t.smpl.len() / 2;
    let pi = t
        .phdr
        .split_last()
        .map(|(_, real)| real)
        .and_then(|real| {
            real.iter().position(|&(preset, bank, _)| match select {
                Some(s) => s.preset == preset && s.bank == bank,
                None => true,
            })
        })
        .ok_or(SamplerError::NoZones)?;
    let (pb0, pb1) = (
        usize::from(t.phdr[pi].2),
        usize::from(t.phdr.get(pi + 1).ok_or(SamplerError::BadIndex)?.2),
    );
    if pb0 > pb1 {
        return Err(SamplerError::BadIndex);
    }

    let mut samples: Vec<SampleData> = Vec::new();
    let mut slot: Vec<Option<u32>> = vec![None; t.shdr.len()];
    let mut zones: Vec<Zone> = Vec::new();
    let mut total_frames = 0usize;
    let mut visits = 0usize;
    let visit_cap = limits.max_zones.saturating_mul(8);
    let mut global_p = GenSet::default();

    for (n, pb) in (pb0..pb1).enumerate() {
        let (pz, link) = zone_of(&t.pbag, &t.pgen, pb, G_INSTRUMENT)?;
        let Some(inst_idx) = link else {
            if n == 0 {
                global_p = pz;
            }
            continue;
        };
        let pset = global_p.over(&pz);
        let inst_idx = usize::from(inst_idx);
        if inst_idx + 1 >= t.inst.len() {
            return Err(SamplerError::BadIndex);
        }
        let (ib0, ib1) = (
            usize::from(t.inst[inst_idx]),
            usize::from(t.inst[inst_idx + 1]),
        );
        if ib0 > ib1 {
            return Err(SamplerError::BadIndex);
        }
        let mut global_i = GenSet::default();
        for (m, ib) in (ib0..ib1).enumerate() {
            visits += 1;
            if visits > visit_cap {
                return Err(SamplerError::TooLarge);
            }
            let (iz, link) = zone_of(&t.ibag, &t.igen, ib, G_SAMPLE_ID)?;
            let Some(sample_id) = link else {
                if m == 0 {
                    global_i = iz;
                }
                continue;
            };
            let ig = global_i.over(&iz);
            let sh = t
                .shdr
                .get(usize::from(sample_id))
                .filter(|_| usize::from(sample_id) + 1 < t.shdr.len())
                .ok_or(SamplerError::BadIndex)?;
            if sh.kind & 0x8000 != 0 {
                continue;
            }
            let Some(keys) = intersect(ig.range(G_KEY_RANGE), pset.range(G_KEY_RANGE)) else {
                continue;
            };
            let Some(vels) = intersect(ig.range(G_VEL_RANGE), pset.range(G_VEL_RANGE)) else {
                continue;
            };
            let add = |op: usize, default: i32| {
                i64::from(ig.int(op).unwrap_or(default)) + i64::from(pset.int(op).unwrap_or(0))
            };
            if sh.start > sh.end || sh.end as usize > smpl_frames {
                return Err(SamplerError::BadIndex);
            }
            let n_frames = (sh.end - sh.start) as usize;
            if n_frames == 0 {
                continue;
            }
            if n_frames > limits.max_sample_frames {
                return Err(SamplerError::TooLarge);
            }
            if sh.rate == 0 || sh.rate > 768_000 {
                return Err(SamplerError::Malformed);
            }
            let index = match slot[usize::from(sample_id)] {
                Some(i) => i,
                None => {
                    total_frames += n_frames;
                    if total_frames > limits.max_total_frames {
                        return Err(SamplerError::TooLarge);
                    }
                    let base = sh.start as usize * 2;
                    let pcm = t
                        .smpl
                        .get(base..base + n_frames * 2)
                        .ok_or(SamplerError::Truncated)?;
                    let data = pcm
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0)
                        .collect();
                    let s = SampleData::new(sh.rate, 1, 0, data)
                        .map_err(|_| SamplerError::Malformed)?;
                    samples.push(s);
                    let i = (samples.len() - 1) as u32;
                    slot[usize::from(sample_id)] = Some(i);
                    i
                }
            };
            let mode = match ig.int(G_SAMPLE_MODES).unwrap_or(0) & 3 {
                1 => LoopMode::Continuous,
                3 => LoopMode::Sustain,
                _ => LoopMode::None,
            };
            let (mut loop_start, mut loop_end) = (0u32, 0u32);
            if mode != LoopMode::None {
                let rel = |abs: u32, fine: i64, coarse: i64| {
                    i64::from(abs) - i64::from(sh.start) + fine + coarse * 32768
                };
                let ls = rel(
                    sh.loop_start,
                    add(G_START_LOOP_FINE, 0),
                    add(G_START_LOOP_COARSE, 0),
                );
                let le = rel(
                    sh.loop_end,
                    add(G_END_LOOP_FINE, 0),
                    add(G_END_LOOP_COARSE, 0),
                );
                if ls < 0 || ls >= le || le > n_frames as i64 {
                    return Err(SamplerError::BadLoop);
                }
                loop_start = ls as u32;
                loop_end = le as u32;
            }
            let root = match ig.int(G_ROOT_KEY) {
                Some(k) if (0..=127).contains(&k) => k as u8,
                _ if sh.root <= 127 => sh.root,
                _ => 60,
            };
            let tune = add(G_COARSE_TUNE, 0) * 100 + add(G_FINE_TUNE, 0) + i64::from(sh.correction);
            let secs = |op: usize| {
                let tc = add(op, -12000).clamp(-12000, 8000) as f64;
                2f64.powf(tc / 1200.0) as f32
            };
            let sustain = pow10(-(add(G_SUSTAIN, 0).clamp(0, 1440) as f64) / 200.0) as f32;
            let gain = pow10(-(add(G_ATTENUATION, 0).clamp(0, 1440) as f64) / 200.0) as f32;
            zones.push(Zone {
                sample: index,
                key_lo: keys.0,
                key_hi: keys.1,
                vel_lo: vels.0,
                vel_hi: vels.1,
                root_key: root,
                tune_cents: tune.clamp(-48_000, 48_000) as i32,
                scale_cents: add(G_SCALE_TUNING, 100).clamp(-1200, 1200) as i16,
                loop_mode: mode,
                loop_start,
                loop_end,
                adsr: Adsr {
                    attack: secs(G_ATTACK),
                    decay: secs(G_DECAY),
                    sustain,
                    release: secs(G_RELEASE),
                },
                gain,
                pan: add(G_PAN, 0).clamp(-500, 500) as f32 / 500.0,
            });
            if zones.len() > limits.max_zones {
                return Err(SamplerError::TooLarge);
            }
        }
    }
    if zones.is_empty() {
        return Err(SamplerError::NoZones);
    }
    Ok(SampleBank::new(samples, zones)?)
}
