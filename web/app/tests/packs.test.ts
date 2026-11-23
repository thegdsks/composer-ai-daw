import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import type { SoundEntry } from '../src/backend/types';
import { PackLoader } from '../src/sounds/packs';
import { availability, browse, licenceLine } from '../src/sounds/catalogue';
import { WasmEditor } from '../src/wasm/editor';
import { wasmModule } from './helpers';

const here = dirname(fileURLToPath(import.meta.url));
const packs = join(here, '..', '..', '..', 'sounds', 'packs');
const have = existsSync(join(packs, 'index.json'));

/** Serves sounds/packs from disk the way the dev server serves the sounds-packs link. */
function disk(url: string): Promise<Response> {
  const file = join(packs, url.replace(/^.*sounds-packs\//, ''));
  return Promise.resolve(existsSync(file) ? new Response(readFileSync(file)) : new Response(null, { status: 404 }));
}

describe('sampled sounds in the browser list', () => {
  it('a wired soundfont entry is ready, a listed one is not', () => {
    const base: SoundEntry = { id: 'x-pack', name: 'X', kind: 'soundfont', licence: 'CC0 1.0', instruments: '', bundle: 'yes' };
    expect(availability({ ...base, status: 'wired', file: 'x-pack/bank.sf2' })).toEqual({ state: 'ready' });
    expect(availability({ ...base, status: 'listed' }).state).toBe('unavailable');
    expect(availability({ ...base, status: 'wired' }).state).toBe('unavailable');
  });

  it.runIf(have)('lists the fetched packs with their licence and loads each bank', async () => {
    const loader = new PackLoader('/sounds-packs/', (u) => disk(u));
    const entries = await loader.entries();
    expect(entries.length).toBeGreaterThanOrEqual(5);
    expect(browse(entries, '', null).readyCount).toBe(entries.length);
    for (const e of entries) {
      expect(licenceLine(e)).toBe('CC0 1.0');
      const ed = WasmEditor.fromModule(wasmModule, 'p');
      expect(await loader.ensure(ed, e.id)).toBe(true);
    }
    expect(await loader.ensure(WasmEditor.fromModule(wasmModule, 'q'), 'not-a-pack')).toBe(false);
  });
});
