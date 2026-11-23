//! The two ways to reach composerd. Neither knows what a command means.

use serde_json::{json, Value};
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

/// What the daemon answered: a result or a JSON-RPC error object.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Result(Value),
    Error(Value),
}

/// Failure to talk to the daemon at all (spawn, socket, framing).
#[derive(Debug)]
pub struct TransportError(pub String);

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TransportError {}

fn err(e: impl fmt::Display) -> TransportError {
    TransportError(e.to_string())
}

pub trait Transport {
    /// Sends one request and returns the reply with the same id. Pushed
    /// notifications seen on the way are skipped.
    fn call(&mut self, method: &str, params: Value) -> Result<Reply, TransportError>;
}

fn request(id: u64, method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn reply_from(msg: &Value) -> Reply {
    match msg.get("error") {
        Some(e) => Reply::Error(e.clone()),
        None => Reply::Result(msg.get("result").cloned().unwrap_or(Value::Null)),
    }
}

/// A private composerd child process speaking JSON lines on stdio.
pub struct StdioTransport {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl StdioTransport {
    pub fn spawn(daemon: &str) -> Result<Self, TransportError> {
        let mut child = Command::new(daemon)
            .args(["--stdio", "--no-ws"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| err(format!("cannot start {daemon}: {e}")))?;
        let stdin = child.stdin.take().ok_or_else(|| err("no child stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| err("no child stdout"))?;
        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }
}

impl Transport for StdioTransport {
    fn call(&mut self, method: &str, params: Value) -> Result<Reply, TransportError> {
        let id = self.next_id;
        self.next_id += 1;
        let stdin = self.stdin.as_mut().ok_or_else(|| err("daemon closed"))?;
        writeln!(stdin, "{}", request(id, method, &params)).map_err(err)?;
        stdin.flush().map_err(err)?;
        loop {
            let mut line = String::new();
            if self.stdout.read_line(&mut line).map_err(err)? == 0 {
                return Err(err("daemon closed the stream"));
            }
            let msg: Value = serde_json::from_str(line.trim()).map_err(err)?;
            if msg.get("id") == Some(&json!(id)) {
                return Ok(reply_from(&msg));
            }
        }
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Closing stdin ends the daemon's read loop; kill only if it lingers.
        self.stdin = None;
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// A client of a running composerd on localhost.
pub struct WsTransport {
    ws: WebSocket<TcpStream>,
    next_id: u64,
}

impl WsTransport {
    /// `url` looks like `ws://127.0.0.1:7420`. The token goes in a header, never the URL.
    pub fn connect(url: &str, token: &str) -> Result<Self, TransportError> {
        let rest = url
            .strip_prefix("ws://")
            .ok_or_else(|| err("only ws:// URLs are supported"))?;
        let host = rest.split('/').next().unwrap_or(rest);
        let stream = TcpStream::connect(host).map_err(err)?;
        let mut req = url.into_client_request().map_err(err)?;
        req.headers_mut().insert(
            "authorization",
            format!("Bearer {token}").parse().map_err(err)?,
        );
        let (ws, _) = tungstenite::client(req, stream).map_err(err)?;
        Ok(Self { ws, next_id: 1 })
    }
}

impl Transport for WsTransport {
    fn call(&mut self, method: &str, params: Value) -> Result<Reply, TransportError> {
        let id = self.next_id;
        self.next_id += 1;
        self.ws
            .send(Message::text(request(id, method, &params).to_string()))
            .map_err(err)?;
        loop {
            match self.ws.read().map_err(err)? {
                Message::Text(t) => {
                    let msg: Value = serde_json::from_str(t.as_str()).map_err(err)?;
                    if msg.get("id") == Some(&json!(id)) {
                        return Ok(reply_from(&msg));
                    }
                }
                Message::Close(_) => return Err(err("daemon closed the socket")),
                _ => {}
            }
        }
    }
}
