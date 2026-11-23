// tungstenite's handshake callback fixes the large `ErrorResponse` error type.
#![allow(clippy::result_large_err)]

use crate::config::Config;
use crate::hub::Hub;
use crate::rpc::{error_line, handle_line, RpcError};
use std::io::{ErrorKind, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tungstenite::error::{CapacityError, Error as WsError};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::Message;

/// Localhost WebSocket listener. Binds 127.0.0.1 only.
pub struct WsServer {
    listener: TcpListener,
    hub: Arc<Hub>,
    cfg: Arc<Config>,
    open: Arc<AtomicUsize>,
}

fn deny(status: u16, why: &str) -> ErrorResponse {
    http::Response::builder()
        .status(status)
        .body(Some(why.to_string()))
        .unwrap_or_else(|_| http::Response::new(None))
}

/// Compares in time independent of where the first difference is.
fn same_secret(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

fn presented_token(req: &Request) -> Option<String> {
    if let Some(auth) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(t) = auth.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    // Browsers cannot set headers on a WebSocket, so the token may ride in the query.
    req.uri()
        .query()?
        .split('&')
        .find_map(|kv| kv.strip_prefix("token=").map(str::to_string))
}

fn host_is_local(req: &Request) -> bool {
    let Some(host) = req.headers().get("host").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => host.split(':').next().unwrap_or(""),
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

/// Rejects the upgrade unless Host, Origin and token all check out.
fn authorize(cfg: &Config, req: &Request) -> Result<(), ErrorResponse> {
    // A Host that is not loopback means DNS rebinding or a proxy.
    if !host_is_local(req) {
        return Err(deny(403, "bad host"));
    }
    if let Some(origin) = req.headers().get("origin") {
        let ok = origin
            .to_str()
            .map(|o| {
                cfg.allowed_origins
                    .iter()
                    .any(|a| a == &o.to_ascii_lowercase())
            })
            .unwrap_or(false);
        if !ok {
            return Err(deny(403, "origin not allowed"));
        }
    }
    match presented_token(req) {
        Some(t) if same_secret(&t, &cfg.token) => Ok(()),
        _ => Err(deny(401, "pairing token required")),
    }
}

struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl WsServer {
    /// Binds 127.0.0.1 on `cfg.port` (0 picks a free port).
    pub fn bind(hub: Arc<Hub>, cfg: Config) -> std::io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, cfg.port)))?;
        Ok(Self {
            listener,
            hub,
            cfg: Arc::new(cfg),
            open: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Accepts connections forever, one thread each.
    pub fn run(self) {
        for stream in self.listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            if self.open.fetch_add(1, Ordering::SeqCst) >= self.cfg.max_clients {
                self.open.fetch_sub(1, Ordering::SeqCst);
                let _ = stream.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
                );
                continue;
            }
            let slot = Slot(self.open.clone());
            let (hub, cfg) = (self.hub.clone(), self.cfg.clone());
            std::thread::spawn(move || {
                let _slot = slot;
                serve_connection(&hub, &cfg, stream);
            });
        }
    }

    /// Runs the accept loop on a background thread.
    pub fn spawn(self) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || self.run())
    }
}

fn serve_connection(hub: &Hub, cfg: &Config, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(cfg.handshake_ms)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(cfg.handshake_ms)));
    let ws_cfg = WebSocketConfig::default()
        .max_message_size(Some(cfg.max_message_bytes))
        .max_frame_size(Some(cfg.max_message_bytes));
    let callback = |req: &Request, resp: Response| authorize(cfg, req).map(|()| resp);
    let Ok(mut ws) = tungstenite::accept_hdr_with_config(stream, callback, Some(ws_cfg)) else {
        return;
    };
    let _ = ws
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(cfg.poll_ms)));
    let Some((client, rx)) = hub.connect() else {
        let _ = ws.close(Some(CloseFrame {
            code: CloseCode::Again,
            reason: "too many clients".into(),
        }));
        let _ = ws.flush();
        return;
    };
    let mut close = None;
    loop {
        while let Ok(msg) = rx.try_recv() {
            if ws.send(Message::text(msg)).is_err() {
                hub.disconnect(&client);
                return;
            }
        }
        match ws.read() {
            Ok(Message::Text(t)) => handle_line(hub, &client, t.as_str()),
            Ok(Message::Binary(_)) => {
                let _ = client.tx.send(error_line(
                    serde_json::Value::Null,
                    &RpcError::invalid_request("messages must be text"),
                ));
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(WsError::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(WsError::Capacity(CapacityError::MessageTooLong { .. })) => {
                close = Some(CloseFrame {
                    code: CloseCode::Size,
                    reason: "message too large".into(),
                });
                break;
            }
            Err(_) => break,
        }
    }
    hub.disconnect(&client);
    let _ = ws.close(close);
    let _ = ws.flush();
}
