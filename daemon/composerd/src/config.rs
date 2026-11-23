use crate::files::FileConfig;
use std::fmt;

/// Runtime settings. Every limit has a default here and an env override.
#[derive(Clone)]
pub struct Config {
    /// `COMPOSERD_PORT`, default 7420, 0 picks a free port.
    pub port: u16,
    /// `COMPOSERD_TOKEN`, at least 16 chars; generated when absent.
    pub token: String,
    /// `COMPOSERD_ALLOWED_ORIGINS`, comma separated. Empty: browsers are refused,
    /// clients that send no Origin header (native tools) are allowed.
    pub allowed_origins: Vec<String>,
    /// `COMPOSERD_MAX_MESSAGE_BYTES`, default 1 MiB, applies to both transports.
    pub max_message_bytes: usize,
    /// `COMPOSERD_MAX_CLIENTS`, default 8, applies to sockets and hub clients.
    pub max_clients: usize,
    /// `COMPOSERD_POLL_MS`, default 25: how often a socket thread flushes pushes.
    pub poll_ms: u64,
    /// `COMPOSERD_HANDSHAKE_MS`, default 5000: time allowed to finish the upgrade.
    pub handshake_ms: u64,
    /// True when the token was generated here, so the binary can show it once.
    pub token_generated: bool,
    /// File and render settings (`COMPOSERD_FILES_DIR` and friends, see [`FileConfig`]).
    pub files: FileConfig,
}

impl fmt::Debug for Config {
    // The token is never printed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .field("allowed_origins", &self.allowed_origins)
            .field("max_message_bytes", &self.max_message_bytes)
            .field("max_clients", &self.max_clients)
            .finish_non_exhaustive()
    }
}

const MIN_TOKEN_LEN: usize = 16;

/// 128 random bits as 32 hex digits.
pub fn generate_token() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("os randomness unavailable");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 7420,
            token: generate_token(),
            allowed_origins: Vec::new(),
            max_message_bytes: 1 << 20,
            max_clients: 8,
            poll_ms: 25,
            handshake_ms: 5000,
            token_generated: true,
            files: FileConfig::default(),
        }
    }
}

fn num<T: std::str::FromStr>(
    get: &impl Fn(&str) -> Option<String>,
    key: &str,
    slot: &mut T,
) -> Result<(), String> {
    if let Some(v) = get(key) {
        *slot = v
            .trim()
            .parse()
            .map_err(|_| format!("{key} is not a valid number"))?;
    }
    Ok(())
}

impl Config {
    /// Reads the process environment.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Builds a config from any key lookup (tests pass a map).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let mut c = Self::default();
        num(&get, "COMPOSERD_PORT", &mut c.port)?;
        num(
            &get,
            "COMPOSERD_MAX_MESSAGE_BYTES",
            &mut c.max_message_bytes,
        )?;
        num(&get, "COMPOSERD_MAX_CLIENTS", &mut c.max_clients)?;
        num(&get, "COMPOSERD_POLL_MS", &mut c.poll_ms)?;
        num(&get, "COMPOSERD_HANDSHAKE_MS", &mut c.handshake_ms)?;
        if let Some(d) = get("COMPOSERD_FILES_DIR") {
            c.files.dir = d.into();
        }
        if let Some(f) = get("COMPOSERD_SOUNDS_FILE") {
            c.files.sounds_file = f.into();
        }
        num(
            &get,
            "COMPOSERD_MAX_FILE_BYTES",
            &mut c.files.max_file_bytes,
        )?;
        num(
            &get,
            "COMPOSERD_MAX_RENDER_SECONDS",
            &mut c.files.max_render_seconds,
        )?;
        num(
            &get,
            "COMPOSERD_RENDER_SAMPLE_RATE",
            &mut c.files.render_sample_rate,
        )?;
        num(
            &get,
            "COMPOSERD_RENDER_TAIL_SECONDS",
            &mut c.files.render_tail_seconds,
        )?;
        num(
            &get,
            "COMPOSERD_MAX_COUNT_IN_BARS",
            &mut c.files.max_count_in_bars,
        )?;
        num(
            &get,
            "COMPOSERD_MAX_HISTORY_PAGE",
            &mut c.files.max_history_page,
        )?;
        num(
            &get,
            "COMPOSERD_GROUP_TIMEOUT_MS",
            &mut c.files.group_timeout_ms,
        )?;
        num(&get, "COMPOSERD_FEED_HISTORY", &mut c.files.feed_history)?;
        num(
            &get,
            "COMPOSERD_MAX_FEED_CHANGES",
            &mut c.files.max_feed_changes,
        )?;
        c.files.audio = composer_audio::AudioLimits::from_lookup(&get);
        if c.files.max_history_page == 0 || c.files.group_timeout_ms == 0 {
            return Err("history page and group timeout must be greater than zero".into());
        }
        if c.files.max_file_bytes == 0
            || c.files.max_render_seconds == 0
            || !(8_000..=192_000).contains(&c.files.render_sample_rate)
        {
            return Err("file and render limits are out of range".into());
        }
        if c.max_message_bytes == 0 || c.max_clients == 0 || c.poll_ms == 0 || c.handshake_ms == 0 {
            return Err("limits must be greater than zero".into());
        }
        if let Some(t) = get("COMPOSERD_TOKEN") {
            if t.len() < MIN_TOKEN_LEN {
                return Err(format!(
                    "COMPOSERD_TOKEN must be at least {MIN_TOKEN_LEN} characters"
                ));
            }
            c.token = t;
            c.token_generated = false;
        }
        if let Some(o) = get("COMPOSERD_ALLOWED_ORIGINS") {
            c.allowed_origins = o
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
        }
        Ok(c)
    }
}
