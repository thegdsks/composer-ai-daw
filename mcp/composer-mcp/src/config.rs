use std::time::Duration;

/// Runtime settings. Every limit has a default here and an env override.
#[derive(Clone)]
pub struct Config {
    /// `COMPOSER_COMPOSERD`: path of the daemon binary (spawn mode).
    pub composerd: Option<String>,
    /// `COMPOSER_WS_URL` and `COMPOSER_TOKEN`: both set selects remote mode.
    pub ws_url: Option<String>,
    pub token: Option<String>,
    /// `COMPOSER_MCP_MAX_MESSAGE_BYTES`, default 1 MiB, one incoming protocol line.
    pub max_message_bytes: usize,
    /// `COMPOSER_MCP_MAX_REPLY_BYTES`, default 64 MiB, one daemon reply.
    pub max_reply_bytes: usize,
    /// `COMPOSER_MCP_MAX_GROUP_COMMANDS`, default 2000.
    pub max_group_commands: usize,
    /// `COMPOSER_MCP_MAX_NEW_IDS`, default 1000 per composer_new_id call.
    pub max_new_ids: usize,
    /// `COMPOSER_MCP_TIMEOUT_MS`, default 30000, wait for one daemon reply.
    pub timeout: Duration,
    /// `COMPOSER_MCP_CACHE_TTL_MS`, default 60000, `ttlMs` hint for the modern era.
    pub cache_ttl_ms: u64,
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

fn text(get: &impl Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    get(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let mut timeout_ms: u64 = 30_000;
        let mut c = Self {
            composerd: text(&get, "COMPOSER_COMPOSERD"),
            ws_url: text(&get, "COMPOSER_WS_URL"),
            token: text(&get, "COMPOSER_TOKEN"),
            max_message_bytes: 1 << 20,
            max_reply_bytes: 64 << 20,
            max_group_commands: 2000,
            max_new_ids: 1000,
            timeout: Duration::ZERO,
            cache_ttl_ms: crate::server::DEFAULT_CACHE_TTL_MS,
        };
        num(&get, "COMPOSER_MCP_CACHE_TTL_MS", &mut c.cache_ttl_ms)?;
        num(
            &get,
            "COMPOSER_MCP_MAX_MESSAGE_BYTES",
            &mut c.max_message_bytes,
        )?;
        num(&get, "COMPOSER_MCP_MAX_REPLY_BYTES", &mut c.max_reply_bytes)?;
        num(
            &get,
            "COMPOSER_MCP_MAX_GROUP_COMMANDS",
            &mut c.max_group_commands,
        )?;
        num(&get, "COMPOSER_MCP_MAX_NEW_IDS", &mut c.max_new_ids)?;
        num(&get, "COMPOSER_MCP_TIMEOUT_MS", &mut timeout_ms)?;
        if c.max_message_bytes == 0
            || c.max_reply_bytes == 0
            || c.max_group_commands == 0
            || c.max_new_ids == 0
            || timeout_ms == 0
        {
            return Err("limits must be greater than zero".into());
        }
        if c.ws_url.is_some() != c.token.is_some() {
            return Err("COMPOSER_WS_URL and COMPOSER_TOKEN must be set together".into());
        }
        c.timeout = Duration::from_millis(timeout_ms);
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_pairing_rule() {
        let c = Config::from_lookup(|_| None).unwrap();
        assert!(c.ws_url.is_none() && c.max_message_bytes == 1 << 20);
        let only_url = |k: &str| (k == "COMPOSER_WS_URL").then(|| "ws://127.0.0.1:1".to_string());
        assert!(Config::from_lookup(only_url).is_err());
        let zero = |k: &str| (k == "COMPOSER_MCP_MAX_NEW_IDS").then(|| "0".to_string());
        assert!(Config::from_lookup(zero).is_err());
    }
}
