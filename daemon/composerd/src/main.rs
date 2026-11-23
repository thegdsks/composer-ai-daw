//! `composerd [--stdio] [--no-ws] [--port N]`. Settings come from `COMPOSERD_*` env vars.

use composerd::{serve_stdio, Config, Hub, WsServer};
use std::io::{stdin, stdout, BufReader};
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let mut cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("composerd: {e}");
            return ExitCode::from(2);
        }
    };
    let (mut use_stdio, mut use_ws) = (false, true);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--stdio" => use_stdio = true,
            "--no-ws" => use_ws = false,
            "--port" => match args.next().and_then(|v| v.parse().ok()) {
                Some(p) => cfg.port = p,
                None => {
                    eprintln!("composerd: --port needs a number");
                    return ExitCode::from(2);
                }
            },
            _ => {
                eprintln!("usage: composerd [--stdio] [--no-ws] [--port N]");
                return ExitCode::from(2);
            }
        }
    }
    if !use_stdio && !use_ws {
        eprintln!("composerd: nothing to serve");
        return ExitCode::from(2);
    }

    let hub = Arc::new(Hub::with_files(cfg.max_clients, cfg.files.clone()));
    let max = cfg.max_message_bytes;
    if use_ws {
        let (token, generated) = (cfg.token.clone(), cfg.token_generated);
        let server = match WsServer::bind(hub.clone(), cfg) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("composerd: cannot bind: {e}");
                return ExitCode::FAILURE;
            }
        };
        if let Ok(addr) = server.local_addr() {
            eprintln!("composerd: websocket on ws://{addr}");
        }
        // A generated token is shown once on stderr so the user can pair; a given one never is.
        if generated {
            eprintln!("composerd: pairing token {token}");
        }
        let handle = server.spawn();
        if !use_stdio {
            let _ = handle.join();
            return ExitCode::SUCCESS;
        }
    }
    match serve_stdio(&hub, max, BufReader::new(stdin()), stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("composerd: stdio ended: {e}");
            ExitCode::FAILURE
        }
    }
}
