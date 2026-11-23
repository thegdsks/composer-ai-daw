/// Size caps for untrusted audio and for decode buffers. Defaults can be overridden from
/// the environment (D41).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioLimits {
    /// Bytes of decode chunk buffers one render may hold at once (COMPOSER_AUDIO_MAX_RESIDENT_BYTES).
    pub max_resident_bytes: u64,
    /// Largest audio file accepted (COMPOSER_AUDIO_MAX_FILE_BYTES).
    pub max_file_bytes: u64,
    /// Most frames in one file (COMPOSER_AUDIO_MAX_FRAMES).
    pub max_frames: u64,
    /// Most channels in one file (COMPOSER_AUDIO_MAX_CHANNELS).
    pub max_channels: u16,
    /// Most bins one peaks query returns (COMPOSER_AUDIO_MAX_PEAK_BINS).
    pub max_peak_bins: u32,
}

impl Default for AudioLimits {
    fn default() -> Self {
        Self {
            max_resident_bytes: 64 << 20,
            max_file_bytes: 16 << 30,
            max_frames: 1 << 33,
            max_channels: 8,
            max_peak_bins: 8192,
        }
    }
}

impl AudioLimits {
    /// Defaults with any valid COMPOSER_AUDIO_MAX_* variable applied.
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Like [`AudioLimits::from_env`] with the variable source injected, for tests.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let num = |name: &str| get(name).and_then(|v| v.trim().parse::<u64>().ok());
        Self {
            max_resident_bytes: num("COMPOSER_AUDIO_MAX_RESIDENT_BYTES")
                .unwrap_or(d.max_resident_bytes),
            max_file_bytes: num("COMPOSER_AUDIO_MAX_FILE_BYTES").unwrap_or(d.max_file_bytes),
            max_frames: num("COMPOSER_AUDIO_MAX_FRAMES").unwrap_or(d.max_frames),
            max_channels: num("COMPOSER_AUDIO_MAX_CHANNELS")
                .and_then(|v| u16::try_from(v).ok())
                .filter(|v| *v >= 1)
                .unwrap_or(d.max_channels),
            max_peak_bins: num("COMPOSER_AUDIO_MAX_PEAK_BINS")
                .and_then(|v| u32::try_from(v).ok())
                .filter(|v| *v >= 1)
                .unwrap_or(d.max_peak_bins),
        }
    }
}
