use crate::automation::{value_at, AutoLane, AutoPoint, LaneParam, CONTROL_PERIOD};
use crate::fx::{Chain, ChainSpec};
use crate::instrument::{Instrument, Voice};
use crate::math::sin_turns;
use crate::mix::{clamp_send, MasterBus, Reverb, MAX_MASTER_VOLUME};

/// Largest block a host may pass to [`Engine::process`] (the Web Audio quantum).
pub const MAX_BLOCK: usize = 128;

/// Capacity of the live event queue (events pushed between two blocks).
pub const LIVE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    NoteOff { pitch: u8 },
    NoteOn { pitch: u8, velocity: u8 },
}

/// A note event at an absolute frame on one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub frame: u64,
    pub track: u32,
    pub kind: EventKind,
}

impl Event {
    /// Order: frame, then offs before ons (so a retrigger frees its voice first), track, pitch.
    fn sort_key(&self) -> (u64, u8, u32, u8) {
        match self.kind {
            EventKind::NoteOff { pitch } => (self.frame, 0, self.track, pitch),
            EventKind::NoteOn { pitch, .. } => (self.frame, 1, self.track, pitch),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    /// Fixed polyphony of every track.
    pub voices_per_track: usize,
}

/// One mixer strip: an instrument plus gain and pan.
#[derive(Debug, Clone)]
pub struct TrackSetup {
    pub instrument: Instrument,
    /// Linear gain.
    pub gain: f32,
    /// -1.0 (left) to 1.0 (right), constant power.
    pub pan: f32,
}

/// Left and right linear gains of a strip for a gain and a pan (-1.0 to 1.0, constant power).
/// The one place the pan law lives, so audio clips mixed outside the engine match its strips.
pub fn pan_gains(gain: f32, pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * 0.125;
    (gain * sin_turns(0.25 - angle), gain * sin_turns(angle))
}

/// Automation lanes of one strip and the values they scale (D36).
struct StripLanes {
    volume: Option<Vec<AutoPoint>>,
    pan: Option<Vec<AutoPoint>>,
    send: Option<Vec<AutoPoint>>,
    /// Linear gain a volume lane of 1000 permille gives (zero for a muted strip).
    gain_base: f32,
    /// Added to the pan lane value.
    pan_base: f32,
}

struct Strip {
    instrument: Instrument,
    /// Static gain and pan from the setup, used by whichever of them has no lane.
    gain: f32,
    pan: f32,
    lanes: Option<Box<StripLanes>>,
    /// Effect chain run after the strip, None for the plain path (D45).
    chain: Option<Chain>,
    gain_l: f32,
    gain_r: f32,
    send: f32,
    voices: Vec<Voice>,
}

/// Deterministic block renderer. All memory is sized in [`Engine::new`].
///
/// Voice stealing: a note on with no idle voice takes the oldest voice that is
/// already releasing, else the oldest held voice (age = start order, cut hard).
/// A note off releases the oldest held voice of that pitch.
pub struct Engine {
    strips: Vec<Strip>,
    events: Vec<Event>,
    cursor: usize,
    live: Vec<Event>,
    frame: u64,
    next_age: u64,
    clipped: u64,
    master: MasterBus,
    master_lane: Option<Vec<AutoPoint>>,
    automated: bool,
    reverb: Reverb,
    master_chain: Option<Chain>,
    sample_rate: u32,
    /// Per strip peak of the last block, post fader and pan, pre master (T27).
    peaks: Vec<f32>,
    /// Master soft clip threshold, linear (D44).
    ceiling: Option<f32>,
}

impl Engine {
    pub fn new(config: EngineConfig, tracks: Vec<TrackSetup>, events: Vec<Event>) -> Self {
        Self::with_rate(config, tracks, events, 48_000)
    }

    /// Like [`Engine::new`] with the sample rate the reverb delay lines are sized for (D26).
    pub fn with_rate(
        config: EngineConfig,
        tracks: Vec<TrackSetup>,
        mut events: Vec<Event>,
        sample_rate: u32,
    ) -> Self {
        // Stable sort keeps same key events in submission order.
        events.sort_by_key(Event::sort_key);
        let n = config.voices_per_track.max(1);
        let peaks = vec![0.0; tracks.len()];
        let strips = tracks
            .into_iter()
            .map(|t| {
                let (gain_l, gain_r) = pan_gains(t.gain, t.pan);
                Strip {
                    instrument: t.instrument,
                    gain: t.gain,
                    pan: t.pan,
                    lanes: None,
                    chain: None,
                    gain_l,
                    gain_r,
                    send: 0.0,
                    voices: vec![Voice::IDLE; n],
                }
            })
            .collect();
        Self {
            strips,
            events,
            cursor: 0,
            live: Vec::with_capacity(LIVE_CAPACITY),
            frame: 0,
            next_age: 0,
            clipped: 0,
            master: MasterBus::default(),
            master_lane: None,
            automated: false,
            reverb: Reverb::new(sample_rate.max(1)),
            master_chain: None,
            sample_rate: sample_rate.max(1),
            peaks,
            ceiling: None,
        }
    }

    /// Sets the linear level above which the master is soft clipped (D44), clamped to
    /// 0.1..=1.0, or `None` for the plain sum (the default, overs only counted).
    pub fn set_ceiling(&mut self, ceiling: Option<f32>) {
        self.ceiling = ceiling.map(|c| c.clamp(0.1, 1.0));
    }

    /// Peak of every track over the last processed block, in track order: the larger of the
    /// left and right absolute value after fader and pan, before reverb, master and clips.
    pub fn track_peaks(&self) -> &[f32] {
        &self.peaks
    }

    /// Sets the master volume and reverb. Takes effect on the next frame, never allocates.
    pub fn set_master(&mut self, master: MasterBus) {
        self.master = MasterBus::new(master.volume, master.reverb_size, master.reverb_mix);
        self.reverb.set_size(self.master.reverb_size);
        if self.master_lane.is_some() {
            self.refresh_automation();
        }
    }

    /// Installs effect chains (D45), replacing the ones in place. Setup time only: it allocates.
    /// Returns false when a chain names an unknown track (that chain is skipped). An empty
    /// chain removes the chain of its owner.
    pub fn set_effects(&mut self, chains: &[ChainSpec]) -> bool {
        let mut ok = true;
        for c in chains {
            let built = (!c.effects.is_empty()).then(|| Chain::new(&c.effects, self.sample_rate));
            match c.track {
                None => self.master_chain = built,
                Some(i) => match self.strips.get_mut(i as usize) {
                    Some(s) => s.chain = built,
                    None => ok = false,
                },
            }
        }
        ok
    }

    /// Sets one parameter of one device without allocating and without resetting its state.
    /// `track` None is the master chain. False for an unknown chain, device or parameter.
    pub fn set_effect_param(
        &mut self,
        track: Option<usize>,
        device: usize,
        param: usize,
        value: i16,
    ) -> bool {
        match self.chain_mut(track) {
            Some(c) => c.set_param(device, param, value),
            None => false,
        }
    }

    /// Switches one device on or off without allocating. False for an unknown chain or device.
    pub fn set_effect_bypass(&mut self, track: Option<usize>, device: usize, bypass: bool) -> bool {
        match self.chain_mut(track) {
            Some(c) => c.set_bypass(device, bypass),
            None => false,
        }
    }

    /// Number of devices in a chain (0 when there is none).
    pub fn effect_count(&self, track: Option<usize>) -> usize {
        match track {
            None => self.master_chain.as_ref(),
            Some(i) => self.strips.get(i).and_then(|s| s.chain.as_ref()),
        }
        .map_or(0, Chain::len)
    }

    fn chain_mut(&mut self, track: Option<usize>) -> Option<&mut Chain> {
        match track {
            None => self.master_chain.as_mut(),
            Some(i) => self.strips.get_mut(i)?.chain.as_mut(),
        }
    }

    /// Installs an automation lane (D36). Setup time only: it allocates. Returns false for an
    /// unknown track. The value of the lane applies from the current frame on.
    pub fn set_lane(&mut self, lane: AutoLane) -> bool {
        let base = lane.base;
        if lane.points.is_empty() {
            return false;
        }
        match (lane.track, lane.param) {
            (None, LaneParam::MasterVolume) => self.master_lane = Some(lane.points),
            (Some(i), param) => {
                let Some(strip) = self.strips.get_mut(i as usize) else {
                    return false;
                };
                let lanes = strip.lanes.get_or_insert_with(|| {
                    Box::new(StripLanes {
                        volume: None,
                        pan: None,
                        send: None,
                        gain_base: 1.0,
                        pan_base: 0.0,
                    })
                });
                match param {
                    LaneParam::Volume => {
                        lanes.gain_base = base;
                        lanes.volume = Some(lane.points);
                    }
                    LaneParam::Pan => {
                        lanes.pan_base = base;
                        lanes.pan = Some(lane.points);
                    }
                    LaneParam::ReverbSend => lanes.send = Some(lane.points),
                    LaneParam::MasterVolume => return false,
                }
            }
            (None, _) => return false,
        }
        self.automated = true;
        self.refresh_automation();
        true
    }

    /// Re-reads every lane at the current frame. Allocation free.
    fn refresh_automation(&mut self) {
        let frame = self.frame;
        if let Some(points) = &self.master_lane {
            self.master.volume = (value_at(points, frame) / 1000.0).clamp(0.0, MAX_MASTER_VOLUME);
        }
        for s in &mut self.strips {
            let Some(l) = &s.lanes else {
                continue;
            };
            if let Some(points) = &l.volume {
                s.gain = l.gain_base * (value_at(points, frame) / 1000.0).max(0.0);
            }
            if let Some(points) = &l.pan {
                s.pan = (l.pan_base + value_at(points, frame) / 1000.0).clamp(-1.0, 1.0);
            }
            if let Some(points) = &l.send {
                s.send = clamp_send(value_at(points, frame) / 1000.0);
            }
            (s.gain_l, s.gain_r) = pan_gains(s.gain, s.pan);
        }
    }

    /// Reverb send of one track, 0.0..=1.0. Returns false for an unknown track.
    pub fn set_send(&mut self, track: usize, send: f32) -> bool {
        match self.strips.get_mut(track) {
            Some(s) => {
                s.send = clamp_send(send);
                true
            }
            None => false,
        }
    }

    /// Queues a note event to sound at the start of the next [`Engine::process`] call.
    /// The queue is preallocated: this never allocates, and returns false when it is full
    /// (the event is dropped). The frame of the event is ignored; live events play now.
    pub fn push_live(&mut self, track: u32, kind: EventKind) -> bool {
        if self.live.len() >= LIVE_CAPACITY {
            return false;
        }
        self.live.push(Event {
            frame: self.frame,
            track,
            kind,
        });
        true
    }

    /// Frame index of the next frame to render.
    pub fn position(&self) -> u64 {
        self.frame
    }

    /// Frames whose master sum exceeded full scale since construction.
    pub fn clipped_frames(&self) -> u64 {
        self.clipped
    }

    /// Pitches of the sounding voices of a track, in voice slot order.
    pub fn voice_pitches(&self, track: usize) -> Vec<u8> {
        self.strips.get(track).map_or_else(Vec::new, |s| {
            s.voices
                .iter()
                .filter(|v| v.active)
                .map(|v| v.pitch)
                .collect()
        })
    }

    /// Renders `left.len().min(right.len())` frames (at most [`MAX_BLOCK`] by contract).
    /// Never allocates. The master is a plain sum unless a ceiling is set (D44); overs are counted before any clip.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_with(left, right, &[], &[]);
    }

    /// Like [`Engine::process`], also adding a stereo block of external audio (clips mixed by
    /// the host, D42) to the voice sum before the master volume. Input shorter than the block
    /// adds nothing for the missing frames; empty input is exactly `process`. Never allocates.
    pub fn process_with(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        in_left: &[f32],
        in_right: &[f32],
    ) {
        let n = left.len().min(right.len());
        debug_assert!(n <= MAX_BLOCK);
        for i in 0..self.live.len() {
            let e = self.live[i];
            self.dispatch(e);
        }
        self.live.clear();
        self.peaks.fill(0.0);
        for i in 0..n {
            if self.automated && self.frame.is_multiple_of(CONTROL_PERIOD) {
                self.refresh_automation();
            }
            while self.cursor < self.events.len() && self.events[self.cursor].frame <= self.frame {
                let e = self.events[self.cursor];
                self.cursor += 1;
                self.dispatch(e);
            }
            let (mut l, mut r) = (0.0f32, 0.0f32);
            let (mut send_l, mut send_r) = (0.0f32, 0.0f32);
            for (s, peak) in self.strips.iter_mut().zip(self.peaks.iter_mut()) {
                let (mut sl, mut sr) = (0.0f32, 0.0f32);
                if let Some(chain) = s.chain.as_mut().filter(|c| c.is_active()) {
                    // Sum the strip first so the chain sees one stereo signal (D45).
                    for v in &mut s.voices {
                        if v.active {
                            let (a, b) = s.instrument.render(v);
                            sl += a * s.gain_l;
                            sr += b * s.gain_r;
                        }
                    }
                    (sl, sr) = chain.process(sl, sr);
                    l += sl;
                    r += sr;
                    send_l += sl * s.send;
                    send_r += sr * s.send;
                    let m = sl.abs().max(sr.abs());
                    if m > *peak {
                        *peak = m;
                    }
                    continue;
                }
                for v in &mut s.voices {
                    if v.active {
                        let (a, b) = s.instrument.render(v);
                        let (a, b) = (a * s.gain_l, b * s.gain_r);
                        l += a;
                        r += b;
                        sl += a;
                        sr += b;
                        send_l += a * s.send;
                        send_r += b * s.send;
                    }
                }
                let m = sl.abs().max(sr.abs());
                if m > *peak {
                    *peak = m;
                }
            }
            if let (Some(a), Some(b)) = (in_left.get(i), in_right.get(i)) {
                l += *a;
                r += *b;
            }
            if self.master.reverb_mix > 0.0 {
                let (wl, wr) = self.reverb.tick(send_l, send_r);
                l += wl * self.master.reverb_mix;
                r += wr * self.master.reverb_mix;
            }
            if let Some(chain) = self.master_chain.as_mut().filter(|c| c.is_active()) {
                (l, r) = chain.process(l, r);
            }
            l *= self.master.volume;
            r *= self.master.volume;
            if l.abs() > 1.0 || r.abs() > 1.0 {
                self.clipped += 1;
            }
            if let Some(c) = self.ceiling {
                l = soft_clip(l, c);
                r = soft_clip(r, c);
            }
            left[i] = l;
            right[i] = r;
            self.frame += 1;
        }
    }

    fn dispatch(&mut self, e: Event) {
        let Some(strip) = self.strips.get_mut(e.track as usize) else {
            return;
        };
        match e.kind {
            EventKind::NoteOff { pitch } => {
                let target = strip
                    .voices
                    .iter_mut()
                    .filter(|v| v.active && v.held && v.pitch == pitch)
                    .min_by_key(|v| v.age);
                if let Some(age) = target.map(|v| v.age) {
                    // Layers of one note share an age and release together.
                    for v in strip
                        .voices
                        .iter_mut()
                        .filter(|v| v.active && v.held && v.pitch == pitch && v.age == age)
                    {
                        strip.instrument.release(v);
                    }
                }
            }
            EventKind::NoteOn { pitch, velocity } => {
                let layers = strip.instrument.layers(pitch, velocity);
                if layers == 0 {
                    return;
                }
                self.next_age += 1;
                for layer in 0..layers {
                    let slot = match strip.voices.iter().position(|v| !v.active) {
                        Some(i) => i,
                        None => steal_slot(&strip.voices),
                    };
                    strip.instrument.start(
                        &mut strip.voices[slot],
                        pitch,
                        velocity,
                        self.next_age,
                        layer,
                    );
                }
            }
        }
    }
}

/// Transparent master limiter (D44): identity up to `ceiling`, then a smooth bend that
/// approaches 1.0 and never reaches it. Plain f32 arithmetic, so native and wasm agree.
fn soft_clip(x: f32, ceiling: f32) -> f32 {
    let a = x.abs();
    if a <= ceiling {
        return x;
    }
    let room = 1.0 - ceiling;
    let t = (a - ceiling) / room.max(f32::MIN_POSITIVE);
    let y = ceiling + room * (t / (1.0 + t));
    if x < 0.0 {
        -y
    } else {
        y
    }
}

/// Suggested soft clip threshold (about -0.09 dBFS) for hosts that enable the limiter (D44).
pub const DEFAULT_CEILING: f32 = 0.99;

/// Oldest releasing voice, else oldest held voice. Voices is never empty.
fn steal_slot(voices: &[Voice]) -> usize {
    let key = |v: &Voice| (v.held, v.age);
    let mut best = 0;
    for (i, v) in voices.iter().enumerate() {
        if key(v) < key(&voices[best]) {
            best = i;
        }
    }
    best
}
