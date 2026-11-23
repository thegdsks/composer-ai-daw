/// Size caps for untrusted files. Defaults can be overridden from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest input file read (COMPOSER_SAMPLER_MAX_FILE_BYTES).
    pub max_file_bytes: usize,
    /// Most frames in one sample (COMPOSER_SAMPLER_MAX_SAMPLE_FRAMES).
    pub max_sample_frames: usize,
    /// Most frames over all samples of one bank (COMPOSER_SAMPLER_MAX_TOTAL_FRAMES).
    pub max_total_frames: usize,
    /// Most zones in one bank (COMPOSER_SAMPLER_MAX_ZONES).
    pub max_zones: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 512 << 20,
            max_sample_frames: 32 << 20,
            max_total_frames: 96 << 20,
            max_zones: 16_384,
        }
    }
}

impl Limits {
    /// Defaults with any valid COMPOSER_SAMPLER_MAX_* variable applied.
    pub fn from_env() -> Self {
        let d = Self::default();
        let get = |name: &str, fallback: usize| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(fallback)
        };
        Self {
            max_file_bytes: get("COMPOSER_SAMPLER_MAX_FILE_BYTES", d.max_file_bytes),
            max_sample_frames: get("COMPOSER_SAMPLER_MAX_SAMPLE_FRAMES", d.max_sample_frames),
            max_total_frames: get("COMPOSER_SAMPLER_MAX_TOTAL_FRAMES", d.max_total_frames),
            max_zones: get("COMPOSER_SAMPLER_MAX_ZONES", d.max_zones),
        }
    }
}
