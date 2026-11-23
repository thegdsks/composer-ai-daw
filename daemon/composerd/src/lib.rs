//! `composerd`: serves one live editor session over JSON-RPC 2.0.
//!
//! Two transports share one [`Hub`]: newline delimited JSON on stdio and a
//! localhost WebSocket guarded by a pairing token and an Origin allowlist.
//! Edits are serialized under one lock, and `state.changed` notifications are
//! queued under that same lock, so every client sees the same order.
//!
//! Methods: `session.new`, `command.apply`, `command.group`, `undo`, `redo`,
//! `state.get`, `state.hash`, `commands.describe`, `subscribe`, and the T17 additions
//! (D16, D17): `project.tree`, `selection.get`, `selection.set`, `transport.get`,
//! `transport.set`, `meters.measure`, `sounds.list`, `project.save`, `project.load`,
//! `export.midi`, `import.midi`, `render.wav`, and the D28 template and recipe methods
//! (`templates.list`, `template.apply`, `recipes.list`, `recipe.plan`, `recipe.apply`), and the D43 audio
//! methods (`audio.import`, `audio.assets`, `audio.peaks`). Mutating methods accept `dry_run`. Errors carry `CommandError::code()` in `data.code`.

#![forbid(unsafe_code)]

mod audio;
mod config;
mod files;
mod hub;
mod methods;
mod proposals;
mod recipes;
mod rpc;
mod stdio;
mod ws;

pub use config::{generate_token, Config};
pub use files::FileConfig;
pub use hub::{ClientHandle, Hub};
pub use rpc::handle_line;
pub use stdio::serve_stdio;
pub use ws::WsServer;
