import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { ComposerDebug } from '../src/debug';
import { Actions } from '../src/state/actions';
import type { Store } from '../src/state/store';
import { WasmBackend } from '../src/backend/wasmBackend';

const here = dirname(fileURLToPath(import.meta.url));

export const wasmBytes = readFileSync(join(here, '..', 'public', 'composer_wasm.wasm'));
export const wasmModule = new WebAssembly.Module(wasmBytes);

/** Actions over a store; the debug sink is only used by WAV export, which tests skip. */
export function newActions(store: Store): Actions {
  return new Actions(store, wasmModule, {} as ComposerDebug, () => undefined);
}

export function counterIds(): () => string {
  let n = 0;
  return () => (++n).toString(16).padStart(32, '0');
}

export function newBackend(name = 'test'): WasmBackend {
  return WasmBackend.create(wasmModule, name);
}
