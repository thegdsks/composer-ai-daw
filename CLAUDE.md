# Composer AI DAW: contributor and agent guide

A beginner-first, AI-drivable music composition app. The project is experimental and released as is (see README.md). This file is for human contributors and for AI coding agents working in the repo.

Licence: AGPL-3.0-only (see LICENSE).

## Architecture

- One headless Rust core: document model, command API, undo, audio engine, offline render.
- The core compiles to native (CLI, daemon, MCP server) and to WASM (browser).
- Every edit is a serializable command. The web app, the CLI and the MCP server are three clients of the same command API. No editor action may exist only in the UI.
- Web UI in TypeScript (React for chrome, canvas for the piano roll and arrangement).
- Plugins: CLAP hosting is planned, not built. Sounds: built-in voices generated in code, SF2 and SFZ loading in the native path, WAV, MIDI.
- Project file: our own JSON format, plus MIDI import and export.

Decisions live in `notes/decisions.yaml`. An entry counts as final only with `status: accepted`. Treat `proposed` as a default to challenge with evidence.

## Reference code rule

The architecture of other open source music software was studied for ideas. Study is read-only: algorithms, data structures, protocols, edge cases and failure modes are free to learn from. Their expression is not.

- Allowed: read reference code, record findings in your own words in `notes/ref-*.yaml` with a `src:` path, design and write our code from those notes.
- Not allowed: pasting or retyping their code, line by line ports, copying their comments, identifiers, file layout, test fixtures, schemas, presets or assets.
- Use our own names (the `we_call_it` glossary in the ref notes), not theirs.
- Record each non-trivial borrowed idea as `inspired_by:` in `notes/decisions.yaml` or in the pull request.
- The reference clones are not part of this repository. Paths in the ref notes are relative to a local checkout that is not published.

## No-hallucination protocol

1. Facts about a reference project come only from `notes/ref-*.yaml`. If a fact is missing, read the reference, add it to the YAML with its `src:` path and lines, then use it.
2. Facts about a library or API: check its current docs before coding against it.
3. A borrowed idea is recorded as `borrow`, `avoid` or `rethink` with its source.
4. A claim of done needs a command that was run and its output. Say what was not run.
5. If two notes disagree, stop and report. Do not pick silently.

## Rules

- No em dashes or en dashes in code, comments, docs or commit messages.
- No `any` in TypeScript, no `console.log` in production code, no inline styles (Tailwind classes and tokens only).
- No hardcoded thresholds: read them from env (see `bench/budgets.env`, `web/app/src/config.ts`).
- No secrets, no `.env` files in commits. No downloaded third-party audio in the repo (see `sounds/`).
- Comments: one to three lines, explain why, not what.
- Commit format: `<type>: <description>` with type in feat, fix, perf, refactor, docs, test, chore.

## Scale budgets

Large projects are a first-class test: many tracks, many regions, long audio, deep undo. `bench/` holds generated stress projects, driven only through the public command API. Thresholds come from env (`bench/budgets.env`, `bench/baseline.yaml`).

## Checks

Run the check for the part you touched.

- Rust: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`. Browser module: `cargo build -p composer-wasm --target wasm32-unknown-unknown`.
- Web app (in `web/app`): `npm run wasm`, `npm run typecheck`, `npm run lint`, `npm test`, `npm run build`.
- Browser scripts (need a Chromium based browser, after `npm run build`): `node e2e/run.mjs <name>`, or `node e2e/smoke.mjs`, `layout.mjs`, `plain.mjs`, `shell.mjs` in `web/app/e2e`.
- Spike determinism check: `node web/spike/build.mjs && node web/spike/test/determinism.mjs`.

## Notes format

Durable notes are YAML in `notes/`. Parse them before committing (`python3 -I -c "import yaml,sys; yaml.safe_load(open(sys.argv[1]))" file`). Quote values that contain `: `. No new markdown piles; findings go in the pull request or an issue.
