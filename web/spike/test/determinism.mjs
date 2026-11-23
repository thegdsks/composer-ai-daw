// Renders the demo project offline through the wasm glue and checks the pinned native hash.
// Run after build.mjs: node web/spike/test/determinism.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { WasmEngine, fnv1a64 } from '../engine.mjs';

const here = dirname(fileURLToPath(import.meta.url));
// Same value as PINNED_HASH in core/composer-render/tests/render.rs.
const PINNED_HASH = 0x9234f99f24d6c39dn;
const SR = 48000;

function renderDemo() {
  const eng = WasmEngine.fromBytes(readFileSync(join(here, '..', 'composer_wasm.wasm')));
  eng.load(eng.demoBlob(SR));
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
  assert.equal(eng.position, total);
  const clipped = eng.clipped;
  eng.free();
  return { out, total, clipped };
}

const a = renderDemo();
const b = renderDemo();
assert.equal(a.total, 204000);
assert.equal(a.out.length, 408000);
assert.equal(a.clipped, 0);
const ha = fnv1a64(a.out);
assert.equal(ha, fnv1a64(b.out), 'two renders differ');
assert.equal(ha, PINNED_HASH, `hash was 0x${ha.toString(16)}`);

// Bad blobs are rejected without trapping.
const eng = WasmEngine.fromBytes(readFileSync(join(here, '..', 'composer_wasm.wasm')));
assert.throws(() => eng.load(new Uint8Array(40)), /invalid blob/);

// A live note after the scheduled material makes sound; silence before it.
eng.load(eng.demoBlob(SR));
for (let i = 0; i < Math.ceil(a.total / 128) + 1; i++) eng.process(128);
const peak = (frames) => Math.max(...frames.flatMap((f) => [...f].map(Math.abs)));
assert.equal(peak(eng.process(128)), 0);
assert.equal(eng.note(0, 69, 100, true), true);
assert.ok(peak(eng.process(128)) > 0);

console.log(`ok: ${a.total} frames, fnv1a64 0x${ha.toString(16)} equals native pinned hash`);
