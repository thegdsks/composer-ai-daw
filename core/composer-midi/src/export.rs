use composer_model::{Project, RegionContent, TrackKind, TICKS_PER_QUARTER};

/// Largest position a variable length delta can carry.
const MAX_SMF_TICK: u64 = 0x0FFF_FFFF;

struct TrackBuf {
    data: Vec<u8>,
    last: u64,
    running: Option<u8>,
}

impl TrackBuf {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            last: 0,
            running: None,
        }
    }

    fn delta(&mut self, tick: u64) {
        let t = tick.clamp(self.last, MAX_SMF_TICK);
        varlen(&mut self.data, (t - self.last) as u32);
        self.last = t;
    }

    fn meta(&mut self, tick: u64, ty: u8, payload: &[u8]) {
        self.delta(tick);
        self.data.extend_from_slice(&[0xFF, ty]);
        varlen(&mut self.data, payload.len() as u32);
        self.data.extend_from_slice(payload);
        self.running = None;
    }

    fn channel(&mut self, tick: u64, status: u8, a: u8, b: u8) {
        self.delta(tick);
        if self.running != Some(status) {
            self.data.push(status);
            self.running = Some(status);
        }
        self.data.extend_from_slice(&[a, b]);
    }

    fn finish(mut self, out: &mut Vec<u8>) {
        let end = self.last;
        self.meta(end, 0x2F, &[]);
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(self.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.data);
    }
}

fn varlen(out: &mut Vec<u8>, v: u32) {
    let mut groups = [0u8; 4];
    let mut n = 0;
    let mut v = v & 0x0FFF_FFFF;
    loop {
        groups[n] = (v & 0x7f) as u8;
        n += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    for i in (0..n).rev() {
        out.push(groups[i] | if i > 0 { 0x80 } else { 0 });
    }
}

/// Channel for the k-th instrument track: 0 to 8, then 10 to 15, repeating.
fn channel_for(k: usize) -> u8 {
    let c = (k % 15) as u8;
    if c >= 9 {
        c + 1
    } else {
        c
    }
}

/// Writes a format 1 file at 960 PPQ: a conductor track (project name, tempo and
/// meter changes) then one track per instrument track. Audio tracks are skipped,
/// as are instrument tracks past the 65534th. Same pitch overlaps cannot be
/// expressed in MIDI and come back paired first on first off.
pub fn export_smf(project: &Project) -> Vec<u8> {
    let tracks: Vec<_> = project
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Instrument)
        .take(usize::from(u16::MAX) - 1)
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&((tracks.len() + 1) as u16).to_be_bytes());
    out.extend_from_slice(&(TICKS_PER_QUARTER as u16).to_be_bytes());

    let mut conductor = TrackBuf::new();
    if !project.name.is_empty() {
        conductor.meta(0, 0x03, project.name.as_bytes());
    }
    // Meter before tempo at the same tick, the usual layout.
    let mut events: Vec<(u64, u8, u8, Vec<u8>)> = Vec::new();
    for (tick, m) in &project.tempo_map.meters {
        let dd = m.denominator.trailing_zeros() as u8;
        events.push((*tick as u64, 0, 0x58, vec![m.numerator, dd, 24, 8]));
    }
    for (tick, mbpm) in &project.tempo_map.tempos {
        let m = u64::from((*mbpm).max(1));
        let us = ((60_000_000_000u64 * 2 + m) / (2 * m)).min(0xFF_FFFF) as u32;
        events.push((*tick as u64, 1, 0x51, us.to_be_bytes()[1..].to_vec()));
    }
    events.sort();
    for (tick, _, ty, payload) in events {
        conductor.meta(tick, ty, &payload);
    }
    conductor.finish(&mut out);

    for (k, t) in tracks.iter().enumerate() {
        let ch = channel_for(k);
        let mut buf = TrackBuf::new();
        buf.meta(0, 0x03, t.name.as_bytes());
        // (tick, is_on, pitch, velocity); offs sort before ons at the same tick.
        let mut ev: Vec<(u64, bool, u8, u8)> = Vec::new();
        for r in t.regions.values() {
            if let RegionContent::Midi { notes } = &r.content {
                for n in notes.values() {
                    let start = (r.start + n.start).max(0) as u64;
                    let end = start + n.duration.max(1) as u64;
                    let pitch = n.pitch & 0x7f;
                    ev.push((start, true, pitch, n.velocity.clamp(1, 127)));
                    ev.push((end, false, pitch, 0));
                }
            }
        }
        ev.sort();
        for (tick, on, pitch, vel) in ev {
            if on {
                buf.channel(tick, 0x90 | ch, pitch, vel);
            } else {
                buf.channel(tick, 0x80 | ch, pitch, 64);
            }
        }
        buf.finish(&mut out);
    }
    out
}
