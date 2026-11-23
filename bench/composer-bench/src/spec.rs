//! Workload sizes and budget thresholds. Every value comes from the environment;
//! see `bench/budgets.env` for the variable list and suggested values.

use std::env;

fn env_u64(name: &str, fallback: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(fallback)
}

fn env_f64(name: &str) -> Option<f64> {
    env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// Shape of one generated stress project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub tracks: u64,
    pub regions_per_track: u64,
    pub notes_per_region: u64,
    /// Undo steps cycled in the undo/redo measurement.
    pub undo_depth: u64,
    pub seed: u64,
}

impl Spec {
    /// Workload sizes: `COMPOSER_BENCH_TRACKS`, `_REGIONS_PER_TRACK`,
    /// `_NOTES_PER_REGION`, `_UNDO_DEPTH`, `_SEED`. The numbers here only size
    /// the workload, they are not pass or fail thresholds.
    pub fn from_env() -> Self {
        Self {
            tracks: env_u64("COMPOSER_BENCH_TRACKS", 300),
            regions_per_track: env_u64("COMPOSER_BENCH_REGIONS_PER_TRACK", 10),
            notes_per_region: env_u64("COMPOSER_BENCH_NOTES_PER_REGION", 50),
            undo_depth: env_u64("COMPOSER_BENCH_UNDO_DEPTH", 10_000),
            seed: env_u64("COMPOSER_BENCH_SEED", 1),
        }
    }

    pub fn regions(&self) -> u64 {
        self.tracks * self.regions_per_track
    }

    pub fn notes(&self) -> u64 {
        self.regions() * self.notes_per_region
    }
}

/// Render workload: `COMPOSER_BENCH_RENDER_TRACKS`, `_RENDER_SECONDS`, `_RENDER_RATE`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderSpec {
    pub tracks: u64,
    pub seconds: u64,
    pub sample_rate: u32,
}

impl RenderSpec {
    pub fn from_env() -> Self {
        Self {
            tracks: env_u64("COMPOSER_BENCH_RENDER_TRACKS", 32),
            seconds: env_u64("COMPOSER_BENCH_RENDER_SECONDS", 300),
            sample_rate: env_u64("COMPOSER_BENCH_RENDER_RATE", 48_000) as u32,
        }
    }
}

/// Upper bounds. A bound that is unset is reported but not enforced.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Budgets {
    pub max_apply_ns: Option<f64>,
    pub max_undo_ns: Option<f64>,
    pub max_redo_ns: Option<f64>,
    pub max_state_hash_ms: Option<f64>,
    pub max_save_ms: Option<f64>,
    pub max_load_ms: Option<f64>,
    /// Render time divided by rendered audio time; 0.1 means 10x faster than real time.
    pub max_render_rtf: Option<f64>,
    /// Lookup cost at the largest size divided by the smallest size.
    pub max_lookup_ratio: Option<f64>,
    /// Same ratio for the cost of one applied command.
    pub max_apply_ratio: Option<f64>,
}

impl Budgets {
    pub fn from_env() -> Self {
        Self {
            max_apply_ns: env_f64("COMPOSER_BENCH_MAX_APPLY_NS"),
            max_undo_ns: env_f64("COMPOSER_BENCH_MAX_UNDO_NS"),
            max_redo_ns: env_f64("COMPOSER_BENCH_MAX_REDO_NS"),
            max_state_hash_ms: env_f64("COMPOSER_BENCH_MAX_STATE_HASH_MS"),
            max_save_ms: env_f64("COMPOSER_BENCH_MAX_SAVE_MS"),
            max_load_ms: env_f64("COMPOSER_BENCH_MAX_LOAD_MS"),
            max_render_rtf: env_f64("COMPOSER_BENCH_MAX_RENDER_RTF"),
            max_lookup_ratio: env_f64("COMPOSER_BENCH_MAX_LOOKUP_RATIO"),
            max_apply_ratio: env_f64("COMPOSER_BENCH_MAX_APPLY_RATIO"),
        }
    }
}
