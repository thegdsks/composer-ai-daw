//! `composer-mcp`: MCP server on stdio. Logs go to stderr, stdout is protocol only.

use composer_mcp::{config::Config, server::Server};
use std::io::{stdin, stdout, BufReader};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("composer-mcp: {e}");
            return ExitCode::from(2);
        }
    };
    let mut server = match Server::start(&cfg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("composer-mcp: {e}");
            return ExitCode::FAILURE;
        }
    };
    match server.run(BufReader::new(stdin()), stdout(), cfg.max_message_bytes) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("composer-mcp: stdio ended: {e}");
            ExitCode::FAILURE
        }
    }
}
