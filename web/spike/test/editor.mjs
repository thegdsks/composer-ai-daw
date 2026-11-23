// Edits through the wasm editor, compiles a blob, renders it and checks the native hash.
// Run after build.mjs: node web/spike/test/editor.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { WasmEngine, fnv1a64 } from '../engine.mjs';
import { WasmEditor } from '../editor.mjs';

const here = dirname(fileURLToPath(import.meta.url));
// Same value as PINNED_HASH in core/composer-wasm/tests/editor.rs.
const PINNED_HASH = 0x459668d8b032cecdn;
const SR = 48000;
const wasm = readFileSync(join(here, '..', 'composer_wasm.wasm'));

// Same sequence as SEQUENCE in the Rust test.
const SEQUENCE = [
  { command: 'add_track', id: 'a', name: 'lead', kind: 'instrument', index: null },
  { command: 'set_instrument', track: 'a', instrument: '2' },
  { command: 'add_region', id: 'b', track: 'a', start: 0, length: 3840, kind: 'midi' },
  { command: 'add_note', id: 'c', region: 'b', pitch: 60, velocity: 100, start: 0, duration: 960 },
  { command: 'add_note', id: 'd', region: 'b', pitch: 64, velocity: 90, start: 960, duration: 960 },
];

function renderBlob(blob) {
  const eng = WasmEngine.fromBytes(wasm);
  eng.load(blob);
  const total = eng.length;
  const out = new Float32Array(total * 2);
  let done = 0;
  while (done < total) {
    const [l, r] = eng.process(Math.min(128, total - done));
    for (let i = 0; i < l.length; i++) {
      out[(done + i) * 2] = l[i];
      out[(done + i) * 2 + 1] = r[i];
    }
    done += l.length;
  }
  eng.free();
  return fnv1a64(out);
}

const ed = WasmEditor.fromBytes(wasm, 'song');
const empty = ed.stateHash;
const hashes = [empty];
for (const cmd of SEQUENCE) {
  const r = ed.apply(cmd);
  assert.ok(r.result, JSON.stringify(r));
  assert.equal(r.result.changed, true);
  assert.equal(r.result.state_hash, ed.stateHash);
  hashes.push(ed.stateHash);
}
const full = ed.stateHash;
assert.equal(new Set(hashes).size, hashes.length, 'every edit changes the hash');
assert.equal(ed.apply(SEQUENCE[0]).error.data.code, 'duplicate_id');

// Undo walks back through the recorded hashes and redo returns to the same ones.
for (let i = SEQUENCE.length - 1; i >= 0; i--) {
  const u = ed.undo();
  assert.equal(u.result.applied, true);
  assert.equal(ed.stateHash, hashes[i]);
}
assert.equal(ed.stateHash, empty);
assert.equal(ed.undo().result.applied, false);
for (let i = 1; i <= SEQUENCE.length; i++) {
  assert.equal(ed.redo().result.applied, true);
  assert.equal(ed.stateHash, hashes[i]);
}
assert.equal(ed.stateHash, full);

// The compiled blob renders to the hash the native Rust path pins.
const blob = ed.engineBlob(SR);
const h = renderBlob(blob);
assert.equal(h, PINNED_HASH, `hash was 0x${h.toString(16)}`);
assert.equal(renderBlob(ed.engineBlob(SR)), h, 'second compile differs');

// Invalid commands return the stable code and leave the hash and history alone.
const depth = ed.undo().result.undo_depth;
ed.redo();
for (const [bad, code] of [
  [{ command: 'delete_track', track: 'ff' }, 'not_found'],
  [{ command: 'add_note', id: 'e', region: 'b', pitch: 200, velocity: 100, start: 0, duration: 10 }, 'invalid'],
  [{ command: 'no_such_command' }, 'invalid_params'],
]) {
  const r = ed.apply(bad);
  assert.equal(r.error.data.code, code, JSON.stringify(r));
  assert.equal(ed.stateHash, full);
}
assert.equal(ed.undo().result.undo_depth, depth);
ed.redo();

// Group is atomic: a failing member rolls the others back and reports its index.
const g = ed.group('two notes', [
  { command: 'add_note', id: 'e', region: 'b', pitch: 67, velocity: 80, start: 1920, duration: 480 },
  { command: 'delete_note', region: 'b', note: 'ee' },
]);
assert.equal(g.error.data.index, 1);
assert.equal(ed.stateHash, full);

// State round trip through load (history cleared, hash preserved).
const state = ed.state().result;
assert.equal(state.state_hash, full);
const other = WasmEditor.fromBytes(wasm, 'other');
const l = other.load(state.project);
assert.equal(l.result.undo_depth, 0);
assert.equal(other.stateHash, full);
assert.ok(ed.describe().result.commands.length >= 20);

ed.free();
other.free();
const size = readFileSync(join(here, '..', 'composer_wasm.wasm')).length;
console.log(`ok: ${SEQUENCE.length} edits, undo/redo, blob renders 0x${h.toString(16)} equals native, wasm ${size} bytes`);
