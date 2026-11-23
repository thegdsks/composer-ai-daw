//! Project mixer state to render settings, and offline level measurement (D14, D17).

use crate::{render_project, RenderSettings, Rendered, TrackMix};
use composer_engine::{any_solo, master_from_project, resolve_mix, send_from_project};
use composer_model::{Id, Project};
use std::collections::BTreeMap;

/// Mixer settings of every track as the project stores them. A muted track, or a
/// track that is not soloed while another one is, gets zero gain.
pub fn mix_from_project(project: &Project) -> BTreeMap<Id, TrackMix> {
    let solo = any_solo(project);
    project
        .tracks
        .iter()
        .map(|t| {
            let (gain, pan) = resolve_mix(&t.mix, solo);
            (t.id, TrackMix { gain, pan })
        })
        .collect()
}

impl RenderSettings {
    /// Takes the per track mixer state, the master bus and the reverb sends from the project,
    /// replacing `mix`, `master` and `sends` (D26).
    pub fn with_project_mix(mut self, project: &Project) -> Self {
        self.mix = mix_from_project(project);
        self.master = master_from_project(project);
        self.sends = project
            .tracks
            .iter()
            .filter(|t| t.reverb_send != 0)
            .map(|t| (t.id, send_from_project(t)))
            .collect();
        self
    }
}

/// Peak and root mean square of one signal, linear (1.0 is full scale), over both channels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub peak: f32,
    pub rms: f32,
}

impl Level {
    fn of(samples: &[f32]) -> Self {
        let mut peak = 0.0f32;
        let mut sum = 0.0f64;
        for &s in samples {
            peak = peak.max(s.abs());
            sum += f64::from(s) * f64::from(s);
        }
        let rms = if samples.is_empty() {
            0.0
        } else {
            (sum / samples.len() as f64).sqrt() as f32
        };
        Self { peak, rms }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeterReport {
    pub sample_rate: u32,
    pub frames: u64,
    pub master: Level,
    pub clipped_frames: u64,
    /// One entry per track in project order, measured with only that track audible.
    pub tracks: Vec<(Id, Level)>,
}

/// Renders offline and reads the levels: the master mix, then each track alone
/// (its own volume, pan and mute apply; solo of other tracks does not). The cost
/// is one render per track plus one.
pub fn measure(project: &Project, settings: &RenderSettings) -> MeterReport {
    let mix = mix_from_project(project);
    let mut master_settings = settings.clone();
    master_settings.mix = mix.clone();
    let master: Rendered = render_project(project, &master_settings);
    let tracks = project
        .tracks
        .iter()
        .map(|t| {
            let mut s = settings.clone();
            // Own mute only: solo elsewhere must not hide a track from its own meter.
            s.mix = project
                .tracks
                .iter()
                .map(|o| {
                    let m = if o.id == t.id {
                        let (gain, pan) = resolve_mix(&o.mix, false);
                        TrackMix { gain, pan }
                    } else {
                        TrackMix {
                            gain: 0.0,
                            pan: 0.0,
                        }
                    };
                    (o.id, m)
                })
                .collect();
            let r = render_project(project, &s);
            (t.id, Level::of(&r.samples))
        })
        .collect();
    MeterReport {
        sample_rate: master.sample_rate,
        frames: master.frames,
        master: Level::of(&master.samples),
        clipped_frames: master.clipped_frames,
        tracks,
    }
}
