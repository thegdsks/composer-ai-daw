//! Streams audio clips into the render in bounded chunks (D42).

use crate::RenderSettings;
use composer_audio::{AudioProvider, AudioStream};
use composer_engine::{pan_gains, TempoClock};
use composer_model::{Id, Project, RegionContent};
use std::sync::Arc;

/// Smallest decode chunk in frames. A clip that cannot get this much of the budget is skipped.
const MIN_CHUNK_FRAMES: u64 = 256;
/// Largest decode chunk in frames, so one clip never grows past a sensible read size.
const MAX_CHUNK_FRAMES: u64 = 1 << 16;

/// A clip that stayed silent, and why. The render itself never fails because of audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioIssue {
    pub region: Id,
    /// `asset_missing`, `no_audio_source`, `over_budget`, `decode_failed` or a decoder code.
    pub code: &'static str,
}

struct Player {
    stream: Box<dyn AudioStream>,
    rate: u64,
    channels: usize,
    frames: u64,
    buf: Vec<f32>,
    buf_start: u64,
    buf_len: u64,
    chunk_frames: u64,
    bytes: u64,
}

impl Player {
    /// Makes frame `idx` and, when it exists, `idx + 1` readable from the buffer.
    /// Returns false when `idx` is past the end of the source or a read failed.
    fn ensure(&mut self, idx: u64) -> Result<bool, ()> {
        if idx >= self.frames {
            return Ok(false);
        }
        let last = (idx + 1).min(self.frames - 1);
        let have = idx >= self.buf_start && last < self.buf_start + self.buf_len;
        if !have {
            let want = self.chunk_frames.min(self.frames - idx) as usize;
            let n = self
                .stream
                .read(idx, &mut self.buf[..want * self.channels])
                .map_err(|_| ())?;
            if n == 0 {
                return Err(());
            }
            self.buf_start = idx;
            self.buf_len = n as u64;
        }
        Ok(true)
    }

    fn frame(&self, idx: u64) -> (f32, f32) {
        let at = ((idx - self.buf_start) as usize) * self.channels;
        let a = self.buf[at];
        let b = if self.channels > 1 {
            self.buf[at + 1]
        } else {
            a
        };
        (a, b)
    }
}

struct Slot {
    region: Id,
    start_frame: u64,
    end_frame: u64,
    asset: String,
    offset: u64,
    gain_l: f32,
    gain_r: f32,
    player: Option<Player>,
}

pub(crate) struct ClipMixer {
    slots: Vec<Slot>,
    /// Index of the first slot not yet started (slots are sorted by start frame).
    next: usize,
    out_rate: u64,
    provider: Option<Arc<dyn AudioProvider>>,
    budget: u64,
    share: u64,
    resident: u64,
    pub(crate) high_water: u64,
    pub(crate) issues: Vec<AudioIssue>,
    pub(crate) end_frame: u64,
}

impl ClipMixer {
    /// None when the project has no audio clip that could make sound.
    pub(crate) fn new(project: &Project, settings: &RenderSettings, sr: u32) -> Option<Self> {
        let clock = TempoClock::new(&project.tempo_map, sr);
        let mut slots = Vec::new();
        for t in &project.tracks {
            let mix = settings.mix.get(&t.id).copied().unwrap_or_default();
            if mix.gain <= 0.0 {
                continue;
            }
            let (pl, pr) = pan_gains(mix.gain, mix.pan);
            for r in t.regions.values() {
                let RegionContent::Clip {
                    asset,
                    offset,
                    gain,
                } = &r.content
                else {
                    continue;
                };
                let g = f32::from(*gain) / 1000.0;
                let start_frame = clock.frame_at(r.start);
                let end_frame = clock.frame_at(r.start + r.length);
                if end_frame <= start_frame || *gain == 0 {
                    continue;
                }
                slots.push(Slot {
                    region: r.id,
                    start_frame,
                    end_frame,
                    asset: asset.clone(),
                    offset: *offset,
                    gain_l: g * pl,
                    gain_r: g * pr,
                    player: None,
                });
            }
        }
        if slots.is_empty() {
            return None;
        }
        slots.sort_by_key(|s| (s.start_frame, s.region));
        let end_frame = slots.iter().map(|s| s.end_frame).max().unwrap_or(0);
        let budget = settings.audio_limits.max_resident_bytes;
        let share = budget / max_overlap(&slots).max(1);
        Some(Self {
            slots,
            next: 0,
            out_rate: u64::from(sr),
            provider: settings.audio.clone(),
            budget,
            share,
            resident: 0,
            high_water: 0,
            issues: Vec::new(),
            end_frame,
        })
    }

    fn open(&mut self, i: usize) {
        let issue = |m: &mut Self, code| {
            m.issues.push(AudioIssue {
                region: m.slots[i].region,
                code,
            });
        };
        let Some(provider) = self.provider.clone() else {
            return issue(self, "no_audio_source");
        };
        let stream = match provider.open(&self.slots[i].asset) {
            Ok(s) => s,
            Err(e) => return issue(self, e.code()),
        };
        let info = stream.info();
        let channels = usize::from(info.channels.max(1));
        let per_frame = channels as u64 * 4;
        let chunk_frames = (self.share / per_frame).clamp(MIN_CHUNK_FRAMES, MAX_CHUNK_FRAMES);
        let chunk_frames = chunk_frames.min(info.frames.max(1));
        let bytes = chunk_frames * per_frame;
        if self.resident + bytes > self.budget {
            return issue(self, "over_budget");
        }
        self.resident += bytes;
        self.high_water = self.high_water.max(self.resident);
        self.slots[i].player = Some(Player {
            stream,
            rate: u64::from(info.sample_rate.max(1)),
            channels,
            frames: info.frames,
            buf: vec![0.0; (chunk_frames as usize) * channels],
            buf_start: 0,
            buf_len: 0,
            chunk_frames,
            bytes,
        });
    }

    /// Mixes the clips sounding in frames `frame0..frame0 + n` into `out_l` and `out_r`
    /// (overwritten). Opens a clip when it starts and closes it when it ends.
    pub(crate) fn mix_block(&mut self, frame0: u64, out_l: &mut [f32], out_r: &mut [f32]) {
        let n = out_l.len().min(out_r.len());
        out_l[..n].fill(0.0);
        out_r[..n].fill(0.0);
        let frame1 = frame0 + n as u64;
        // Close finished clips first so their buffers count as free for the ones starting now.
        for s in &mut self.slots {
            if s.end_frame <= frame0 {
                if let Some(p) = s.player.take() {
                    self.resident -= p.bytes;
                }
            }
        }
        while self.next < self.slots.len() && self.slots[self.next].start_frame < frame1 {
            let i = self.next;
            self.next += 1;
            if self.slots[i].end_frame > frame0 {
                self.open(i);
            }
        }
        let out_rate = self.out_rate;
        for s in &mut self.slots[..self.next] {
            let Some(p) = s.player.as_mut() else { continue };
            let from = s.start_frame.max(frame0);
            let to = s.end_frame.min(frame1);
            let mut failed = false;
            for k in from..to {
                let num = (k - s.start_frame) * p.rate;
                let idx = s.offset + num / out_rate;
                let ok = match p.ensure(idx) {
                    Ok(v) => v,
                    Err(()) => {
                        failed = true;
                        break;
                    }
                };
                if !ok {
                    // Past the end of the source: the rest of the clip is silence.
                    break;
                }
                let (mut a, mut b) = p.frame(idx);
                let rem = num % out_rate;
                if rem != 0 && idx + 1 < p.frames {
                    let (a1, b1) = p.frame(idx + 1);
                    let f = rem as f32 / out_rate as f32;
                    a += (a1 - a) * f;
                    b += (b1 - b) * f;
                }
                let o = (k - frame0) as usize;
                out_l[o] += a * s.gain_l;
                out_r[o] += b * s.gain_r;
            }
            if failed {
                self.issues.push(AudioIssue {
                    region: s.region,
                    code: "decode_failed",
                });
                if let Some(p) = s.player.take() {
                    self.resident -= p.bytes;
                }
            }
        }
    }
}

/// Most clips that overlap at once, by sweeping starts and ends.
fn max_overlap(slots: &[Slot]) -> u64 {
    let mut ends: Vec<u64> = Vec::new();
    let mut best = 0usize;
    for s in slots {
        ends.retain(|e| *e > s.start_frame);
        ends.push(s.end_frame);
        best = best.max(ends.len());
    }
    best as u64
}
