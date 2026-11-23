use crate::config::Config;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Why a daemon call failed.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// The daemon answered with a JSON-RPC error. `code` is its stable `data.code`.
    Remote {
        code: String,
        message: String,
        index: Option<u64>,
    },
    /// We could not reach the daemon or understand it.
    Transport { code: &'static str, message: String },
}

fn transport(code: &'static str, message: impl Into<String>) -> CallError {
    CallError::Transport {
        code,
        message: message.into(),
    }
}

/// A connection to one composerd session. Calls are strictly one at a time.
pub trait Backend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError>;
    /// Releases the connection and reaps any child. Safe to call twice.
    fn shutdown(&mut self);
}

pub enum Line {
    Eof,
    Text(String),
    TooLong,
}

/// Reads one line of at most `max` bytes; an oversized line is skipped to its end.
pub fn read_bounded(r: &mut impl BufRead, max: usize) -> std::io::Result<Line> {
    let mut buf = Vec::new();
    let limit = max as u64 + 1;
    r.by_ref().take(limit).read_until(b'\n', &mut buf)?;
    if buf.is_empty() {
        return Ok(Line::Eof);
    }
    let ended = buf.last() == Some(&b'\n');
    if ended {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
    if buf.len() <= max {
        return Ok(Line::Text(String::from_utf8_lossy(&buf).into_owned()));
    }
    // Over the limit: drop the rest of the line without keeping it.
    if !ended {
        loop {
            buf.clear();
            r.by_ref().take(limit).read_until(b'\n', &mut buf)?;
            if buf.is_empty() || buf.last() == Some(&b'\n') {
                break;
            }
        }
    }
    Ok(Line::TooLong)
}

/// Splits a daemon reply into its result or a typed error.
fn reply_to_result(msg: &Value) -> Result<Value, CallError> {
    if let Some(e) = msg.get("error") {
        let data = e.get("data");
        return Err(CallError::Remote {
            code: data
                .and_then(|d| d.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("daemon_error")
                .to_string(),
            message: e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("daemon error")
                .to_string(),
            index: data.and_then(|d| d.get("index")).and_then(Value::as_u64),
        });
    }
    msg.get("result")
        .cloned()
        .ok_or_else(|| transport("bad_reply", "daemon reply has neither result nor error"))
}

fn request(id: u64, method: &str, params: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

/// Spawn mode: `composerd --stdio --no-ws` as a child, one JSON message per line.
pub struct SpawnBackend {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<Result<Line, String>>,
    next_id: u64,
    timeout: Duration,
}

/// Where the daemon binary is: env, else next to this binary, else on PATH.
pub fn find_composerd(cfg: &Config) -> PathBuf {
    if let Some(p) = &cfg.composerd {
        return PathBuf::from(p);
    }
    let name = format!("composerd{}", std::env::consts::EXE_SUFFIX);
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    {
        let sibling = dir.join(&name);
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from(name)
}

impl SpawnBackend {
    pub fn start(cfg: &Config) -> Result<Self, String> {
        let path = find_composerd(cfg);
        let mut child = Command::new(&path)
            .args(["--stdio", "--no-ws"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Daemon logs share our stderr, stdout stays protocol only.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                format!(
                    "cannot start composerd at {} ({e}); set COMPOSER_COMPOSERD",
                    path.display()
                )
            })?;
        eprintln!("composer-mcp: composerd started, pid {}", child.id());
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or("composerd stdout unavailable")?;
        let (tx, rx) = mpsc::channel();
        let max = cfg.max_reply_bytes;
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            loop {
                let item = read_bounded(&mut r, max).map_err(|e| e.to_string());
                let stop = matches!(item, Ok(Line::Eof) | Err(_));
                if tx.send(item).is_err() || stop {
                    return;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            rx,
            next_id: 1,
            timeout: cfg.timeout,
        })
    }
}

impl Backend for SpawnBackend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        let id = self.next_id;
        self.next_id += 1;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| transport("daemon_unavailable", "composerd is not running"))?;
        writeln!(stdin, "{}", request(id, method, &params))
            .and_then(|()| stdin.flush())
            .map_err(|_| transport("daemon_unavailable", "composerd closed its input"))?;
        let deadline = Instant::now() + self.timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(Ok(Line::Text(t))) => {
                    let Ok(msg) = serde_json::from_str::<Value>(&t) else {
                        continue;
                    };
                    // Pushes (state.changed) and stale replies carry another id.
                    if msg.get("id") == Some(&json!(id)) {
                        return reply_to_result(&msg);
                    }
                }
                Ok(Ok(Line::TooLong)) => {
                    return Err(transport(
                        "reply_too_large",
                        "daemon reply exceeds the limit",
                    ))
                }
                Ok(Ok(Line::Eof)) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(transport("daemon_unavailable", "composerd exited"))
                }
                Ok(Err(e)) => return Err(transport("daemon_unavailable", e)),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(transport("timeout", "composerd did not answer in time"))
                }
            }
        }
    }

    fn shutdown(&mut self) {
        // Closing stdin is the daemon's cue to exit; kill only if it ignores that.
        self.stdin = None;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for SpawnBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Remote mode: an authenticated WebSocket to a running composerd.
pub struct WsBackend {
    ws: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
}

impl WsBackend {
    pub fn connect(url: &str, token: &str, timeout: Duration) -> Result<Self, String> {
        use tungstenite::client::IntoClientRequest;
        let rest = url
            .strip_prefix("ws://")
            .ok_or("COMPOSER_WS_URL must start with ws:// (composerd is local only)")?;
        let host_port = rest.split('/').next().unwrap_or(rest);
        let mut req = format!("ws://{host_port}/")
            .into_client_request()
            .map_err(|_| "COMPOSER_WS_URL is not a valid URL".to_string())?;
        // Bearer header, so the secret never appears in a URL or a log line.
        let bearer = format!("Bearer {token}")
            .parse()
            .map_err(|_| "COMPOSER_TOKEN has invalid characters".to_string())?;
        req.headers_mut().insert("authorization", bearer);
        let stream = TcpStream::connect(host_port)
            .map_err(|e| format!("cannot connect to {host_port}: {e}"))?;
        let _ = stream.set_nodelay(true);
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .map_err(|e| e.to_string())?;
        let (ws, _) = tungstenite::client(req, stream)
            .map_err(|_| "websocket handshake refused (check COMPOSER_TOKEN)".to_string())?;
        Ok(Self { ws, next_id: 1 })
    }
}

impl Backend for WsBackend {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        use tungstenite::{Error, Message};
        let id = self.next_id;
        self.next_id += 1;
        self.ws
            .send(Message::text(request(id, method, &params)))
            .map_err(|_| transport("daemon_unavailable", "websocket send failed"))?;
        loop {
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    let Ok(msg) = serde_json::from_str::<Value>(t.as_str()) else {
                        continue;
                    };
                    if msg.get("id") == Some(&json!(id)) {
                        return reply_to_result(&msg);
                    }
                }
                Ok(Message::Close(_)) | Err(Error::ConnectionClosed | Error::AlreadyClosed) => {
                    return Err(transport("daemon_unavailable", "websocket closed"))
                }
                Ok(_) => {}
                Err(Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(transport("timeout", "composerd did not answer in time"))
                }
                Err(_) => return Err(transport("daemon_unavailable", "websocket read failed")),
            }
        }
    }

    fn shutdown(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}

/// Picks the transport from config: remote when both URL and token are set.
pub fn connect(cfg: &Config) -> Result<Box<dyn Backend>, String> {
    match (&cfg.ws_url, &cfg.token) {
        (Some(u), Some(t)) => Ok(Box::new(WsBackend::connect(u, t, cfg.timeout)?)),
        _ => Ok(Box::new(SpawnBackend::start(cfg)?)),
    }
}
