//! MCP server over stdio that drives a composerd session. No editing logic lives
//! here: every tool becomes one JSON-RPC call to the daemon.

pub mod backend;
pub mod config;
pub mod schema;
pub mod server;
pub mod summary;
pub mod tools;
