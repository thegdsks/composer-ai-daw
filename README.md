# Composer AI DAW

A beginner-first, AI-drivable music composition app: a headless Rust core, a web app, a daemon, a CLI and an MCP server, all driven by one command API.

> **Status: EXPERIMENTAL. Released as is.**
> This project is not actively maintained and may be continued in the future. Expect rough edges. There is no support promise, no roadmap commitment and no promise to review issues or pull requests. It is published so that the code, the design notes and the ideas are available to anyone who finds them useful.

## What it is

You open the web app, pick a starting point (a template, or a blank page), add notes on a piano roll, hear them through built-in voices, and save the result. The same project can be edited by a script or an AI client, because every edit is a serializable command and the web app, the CLI and the MCP server are all clients of that one command API. Nothing an editor can do exists only in the UI.

## What works today

This list is grounded in the code and in `notes/tasks.yaml` and `notes/handoff.yaml`. "Works" means it is implemented and covered by tests or browser scripts, not that it has been judged by ear (see the next section).

- Piano roll on a canvas: add, move, resize and delete notes, multi select, bulk edit, velocity lane, quantize, note list, key shading and stay in key.
- Arrangement view: tracks and clips, split, duplicate, move between tracks, copy, cut, paste and repeat, loop length per clip, markers, tempo and key lanes.
- Mixer with level meters, volume, pan, mute, solo and a master reverb.
- Built-in voices generated in code (no third party audio), and a sound browser that picks a voice for a track and plays a preview.
- Starter templates and recipes (generated idea starters) as command data, plus a start screen and a first run card.
- Key and scale tools: set the key, conform notes to the key, chord helper and chord strip.
- Record from the computer keyboard or a MIDI keyboard.
- Transport with loop, metronome, count in and follow.
- History panel and a command palette, with undo and redo for everything.
- Project files in our own JSON format, autosave in the browser, MIDI import and export, and offline WAV render.
- Audio clips: import a WAV as an asset and place it on an audio track (native path and project model).
- A proposals channel: an AI client can offer an edit, the person previews it and accepts it as one undo step.
- A daemon with JSON-RPC over stdio and a local WebSocket, a CLI that replays scripts, and an MCP server whose tools map to the commands.
- Stress project generator and scale benchmarks (`bench/`).

## What is NOT verified

Stated plainly, so nobody is surprised:

- Nobody has done listening checks on the built-in voices, the templates or the generated music. They may sound poor.
- Safari and Firefox are untested. Use a Chromium based browser.
- Screen reader support is untested, although the app announces edits in a live region.
- Real MIDI hardware is untested.
- Plugin hosting (CLAP) is not built.
- Browser playback of audio clips is not built.
- Loading sample packs (SF2, SFZ) in the browser is not built. The native render path can read them.
- The sound catalogue licences were only partly read. No third party audio is stored in this repository.

## Architecture

```
            +-------------------------------------------+
            |  core crates (Rust)                       |
            |  model, commands (the command API), undo, |
            |  theory, engine, render, format, midi,    |
            |  sampler, audio, recipes, catalogue       |
            +--------+------------------+---------------+
                     |                  |
          compiled to WASM        native binaries
                     |                  |
              composer-wasm      composerd (daemon: JSON-RPC over
                     |            stdio and WebSocket)
                     |                  |
                web/app (React        +--+----------------+
                chrome, canvas        |                   |
                roll, AudioWorklet)  composer (CLI)   composer-mcp
                                                       (MCP server)
```

Key design decisions (full list with reasons in `notes/decisions.yaml`):

- One headless core with a single command API. UI, CLI and MCP are clients of it.
- Edits are invertible commands, so undo, redo, history and AI proposals share one mechanism.
- The same engine code runs natively and in the browser as WASM. Rendering is deterministic: the native and WASM output of the demo project match bit for bit in the Node test.
- Integer time (960 ticks per quarter note) and integer parameters, no floating point in the document.
- The web app talks to a `Backend` interface with a WASM implementation by default and a daemon implementation for driving a live project from outside.

## Quick start

Prerequisites: the Rust toolchain pinned in `rust-toolchain.toml` (it installs the `wasm32-unknown-unknown` target and clippy and rustfmt through rustup), and Node.js with npm.

Web app:

```
cd web/app
npm install
npm run dev
```

Then open http://127.0.0.1:5173 in a Chromium based browser. The first run builds the WASM module (`npm run wasm` does it by hand).

Daemon (JSON-RPC over stdio and a WebSocket on 127.0.0.1:7420; it prints a pairing token on stderr unless `COMPOSERD_TOKEN` is set):

```
cargo run -p composerd
cargo run -p composerd -- --stdio --no-ws
```

CLI (starts its own daemon per command, or connects to one with `--ws`):

```
cargo build -p composerd -p composer-cli
target/debug/composer --daemon target/debug/composerd describe
```

`composer --help` prints the command list. Use `--ws URL` with `COMPOSER_TOKEN` set to keep state across steps in a running daemon, or `run FILE` to replay a script.

MCP server (stdio). It starts a daemon itself when `COMPOSER_COMPOSERD` points at the binary, or connects to a running one with `COMPOSER_WS_URL` and `COMPOSER_TOKEN`. A client configuration example is in `mcp/composer-mcp/example-config.json`.

```
cargo build -p composerd -p composer-mcp
COMPOSER_COMPOSERD=target/debug/composerd target/debug/composer-mcp
```

To watch the web app follow a daemon, open `http://127.0.0.1:5173/?daemon=ws://127.0.0.1:7420#token=<pairing token>`.

## Checks

Rust (run from the repo root):

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo build -p composer-wasm --target wasm32-unknown-unknown
node web/spike/build.mjs && node web/spike/test/determinism.mjs
```

Web app (in `web/app`):

```
npm run wasm
npm run typecheck
npm run lint
npm test
npm run build
```

Browser scripts live in `web/app/e2e` (`smoke.mjs`, `layout.mjs`, `plain.mjs`, `shell.mjs` and many more, or `node e2e/run.mjs <name>`). They build and serve the app on a free port and drive a headless browser through the `agent-browser` command line tool (set `AGENT_BROWSER_BIN` to point at it), so they need that tool and a Chromium based browser installed.

Counts at the time of writing: `cargo test --workspace` ran 450 tests (unit, integration and doc tests), all passing; `npm test` ran 372 tests passing and 1 skipped in 43 files. The browser scripts `smoke`, `layout`, `plain` (8 checks) and `shell` (16 checks) all passed once each.

## Repo layout

| Path | What it is |
| --- | --- |
| `core/` | Rust crates: model, commands, theory, engine, render, format, midi, sampler, audio, recipes, catalogue, wasm |
| `daemon/composerd` | Daemon: JSON-RPC over stdio and WebSocket |
| `cli/composer-cli` | The `composer` command line client |
| `mcp/composer-mcp` | MCP server over the daemon |
| `bench/` | Stress project generator, scale benchmarks, budgets from env |
| `web/app` | The web app (Vite, TypeScript, React chrome, Tailwind, canvas) |
| `web/spike` | Early browser audio spike and determinism test |
| `templates/` | Starter project templates as command data |
| `sounds/` | Sound source catalogue (metadata only) and a fetch script, nothing bundled |
| `notes/` | Decisions, task queue, handoff, reference notes, plans: the shared memory of the project |

## How it was built

It was built with heavy use of AI coding agents working in parallel from a written plan, with the `notes/` folder as the shared memory between them. Each agent worked on its own branch and task, and a person set the direction and the rules. That is why the notes are detailed and why some of them read like instructions to a colleague.

## Reference rule and licence

Composer AI DAW is licensed under AGPL-3.0-only. See `LICENSE`. The Rust crates and the web app declare the same licence in their manifests.

The project studied the architecture of other open source music software for ideas: algorithms, data structures, protocols and failure modes. No source was copied. Findings were written in our own words in `notes/ref-*.yaml`, and the code was written from those notes with our own names. AGPL was chosen partly to stay compatible with that body of work. A dependency licence check is in `notes/dependency-licences.yaml`.

## Contributing

Issues and forks are welcome. There is no promise of review or reply. If you continue the project in a fork, `CLAUDE.md` is the contributor and agent guide, and `notes/handoff.yaml` says how to resume.

## Might continue

If the project resumes, the likely next items from `notes/tasks.yaml` are:

- CLAP plugin hosting through the daemon.
- Browser playback of audio clips.
- Loading sample packs (SF2, SFZ) in the browser.
- A real listening and sound design pass over voices and templates.
- An accessibility audit, including a screen reader pass.
- Performance work on very large projects (the budgets in `bench/` are not enforced by default yet).
