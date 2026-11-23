import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { PreviewCompiler, PREVIEW_CHORD, previewNotes } from '../src/audio/preview';
import type { SoundEntry } from '../src/backend/types';
import { starterCommands } from '../src/fixtures';
import { BUILTIN_ENTRIES, builtinCatalogue } from '../src/sounds/builtin';
import { CATEGORIES, PACK_REASON, attributionOf, availability, browse, categoryOf } from '../src/sounds/catalogue';
import { Store } from '../src/state/store';
import { readBlobInfo, trackTable } from '../src/wasm/blob';
import { WasmEngine } from '../src/wasm/engine';
import { counterIds, newActions, newBackend, wasmModule } from './helpers';

const here = dirname(fileURLToPath(import.meta.url));

/** Flat reader for the catalogue file: top level entry fields that are single line scalars. */
function readCatalogue(): Record<string, string>[] {
  const text = readFileSync(join(here, '..', '..', '..', 'sounds', 'catalogue.yaml'), 'utf8');
  const out: Record<string, string>[] = [];
  for (const line of text.split('\n')) {
    const start = line.match(/^ {2}- id: (.*)$/);
    if (start) {
      out.push({ id: unquote(start[1] ?? '') });
      continue;
    }
    const kv = line.match(/^ {4}([a-z_]+): (.*)$/);
    const cur = out[out.length - 1];
    if (kv && cur) cur[kv[1] ?? ''] = unquote(kv[2] ?? '');
  }
  return out;
}

function unquote(s: string): string {
  return s.replace(/^"(.*)"$/, '$1');
}

function entry(over: Partial<SoundEntry>): SoundEntry {
  return { id: 'x', name: 'X', kind: 'sfz', licence: 'CC0 1.0', instruments: '', bundle: 'yes', ...over };
}

describe('built in voice list', () => {
  it('matches the builtin entries of sounds/catalogue.yaml', () => {
    const file = readCatalogue().filter((e) => e['kind'] === 'builtin');
    expect(file).toHaveLength(16);
    expect(BUILTIN_ENTRIES.map((e) => e.id)).toEqual(file.map((e) => e['id']));
    for (const e of BUILTIN_ENTRIES) {
      const f = file.find((x) => x['id'] === e.id);
      expect(f?.['name']).toBe(e.name);
      expect(f?.['kind']).toBe(e.kind);
      expect(f?.['licence']).toBe(e.licence);
      expect(f?.['bundle']).toBe(e.bundle);
    }
  });

  it('is what the in-browser backend lists', async () => {
    const r = await newBackend().sounds();
    expect(r.ok && r.value).toEqual(builtinCatalogue());
  });
});

describe('catalogue entry to category mapping', () => {
  it('puts every built in voice in a named category and uses each category', () => {
    const cats = BUILTIN_ENTRIES.map((e) => categoryOf(e));
    expect([...new Set(cats)].sort()).toEqual([...CATEGORIES].sort());
    expect(categoryOf(BUILTIN_ENTRIES[0]!)).toBe('Keys');
  });

  it('files sample packs by plain words and falls back to More', () => {
    expect(categoryOf(entry({ name: 'FreePats Synth Pad Choir', instruments: 'choir pad' }))).toBe('Choir');
    expect(categoryOf(entry({ name: 'Upright Piano', instruments: 'upright piano' }))).toBe('Keys');
    expect(categoryOf(entry({ name: 'Drum Kit', instruments: 'drums' }))).toBe('More');
  });
});

describe('hidden and unavailable rules', () => {
  it('shows built ins as ready and sample packs as unavailable with a reason', () => {
    expect(availability(BUILTIN_ENTRIES[0]!)).toEqual({ state: 'ready' });
    expect(availability(entry({ kind: 'sfz' }))).toEqual({ state: 'unavailable', reason: PACK_REASON });
    expect(availability(entry({ kind: 'soundfont', bundle: 'maybe' })).state).toBe('unavailable');
  });

  it('hides what can never play: not shippable, plugins, loose samples, bad ids', () => {
    expect(availability(entry({ kind: 'sfz', bundle: 'no' })).state).toBe('hidden');
    expect(availability(entry({ kind: 'synth' })).state).toBe('hidden');
    expect(availability(entry({ kind: 'samples' })).state).toBe('hidden');
    expect(availability(entry({ kind: 'builtin', id: 'Bad Id' })).state).toBe('hidden');
  });

  it('applies the rules over the real catalogue file', () => {
    const all = readCatalogue().map((e) =>
      entry({ id: e['id'] ?? '', name: e['name'] ?? '', kind: e['kind'] ?? '', bundle: e['bundle'] ?? '', instruments: e['instruments'] ?? '' }),
    );
    const r = browse(all, '', null);
    expect(r.readyCount).toBe(16);
    expect(r.packs.length).toBeGreaterThan(0);
    const ids = new Set([...r.groups.flatMap((g) => g.ready), ...r.packs.map((p) => p.entry)].map((e) => e.id));
    expect(ids.has('surge-xt')).toBe(false);
    expect(ids.has('freesound-cc0')).toBe(false);
    expect(all.filter((e) => e.bundle === 'no').every((e) => !ids.has(e.id))).toBe(true);
  });

  it('filters by search words and by category', () => {
    const cat = builtinCatalogue().entries;
    expect(browse(cat, 'warm', null).readyCount).toBe(2);
    expect(browse(cat, 'warm pad', null).groups.map((g) => g.category)).toEqual(['Pads']);
    expect(browse(cat, '', 'Bells').groups.flatMap((g) => g.ready.map((e) => e.name))).toEqual(['Bell', 'Mallet']);
    expect(browse(cat, 'zzz', null).readyCount).toBe(0);
  });

  it('discloses attribution only when it is required', () => {
    expect(attributionOf(BUILTIN_ENTRIES[0]!)).toBeNull();
    expect(attributionOf(entry({ attribution_required: 'yes', attribution_text: 'Credit A' }))).toBe('Credit A');
  });
});

describe('picking a sound', () => {
  async function seeded() {
    const store = new Store(newBackend());
    await store.refresh();
    await store.group('starter', starterCommands(counterIds()));
    const track = store.getSnapshot().project.tracks[0]?.id ?? '';
    return { store, actions: newActions(store), track };
  }

  it('sets the sound as one undo step and undo clears it', async () => {
    const { store, actions, track } = await seeded();
    const depth = store.getSnapshot().undoDepth;
    expect(await actions.setSound(track, 'builtin-warm-pad')).toBe(true);
    expect(store.getSnapshot().project.tracks[0]?.sound).toBe('builtin-warm-pad');
    expect(store.getSnapshot().undoDepth).toBe(depth + 1);
    expect(await actions.setSound(track, 'builtin-warm-pad')).toBe(false);
    expect(store.getSnapshot().undoDepth).toBe(depth + 1);
    await store.undo();
    expect(store.getSnapshot().project.tracks[0]?.sound).toBeUndefined();
  });

  it('picking a basic instrument clears the sound in the same step', async () => {
    const { store, actions, track } = await seeded();
    await actions.setSound(track, 'builtin-bell');
    const depth = store.getSnapshot().undoDepth;
    await actions.setInstrument(track, '00000000000000000000000000000002');
    const t = store.getSnapshot().project.tracks[0];
    expect(t?.sound).toBeUndefined();
    expect(t?.instrument).toBe('00000000000000000000000000000002');
    expect(store.getSnapshot().undoDepth).toBe(depth + 1);
  });

  it('changes the state hash and the engine track table, which is what triggers the blob rebuild', async () => {
    const { store, actions, track } = await seeded();
    const hash0 = store.getSnapshot().stateHash;
    const b0 = await store.backend.engineBlob(48000);
    await actions.setSound(track, 'builtin-warm-pad');
    const hash1 = store.getSnapshot().stateHash;
    const b1 = await store.backend.engineBlob(48000);
    await actions.setSound(track, 'builtin-bell');
    const b2 = await store.backend.engineBlob(48000);
    if (!b0.ok || !b1.ok || !b2.ok) throw new Error('blob');
    expect(hash1).not.toBe(hash0);
    expect(Array.from(trackTable(b1.value))).not.toEqual(Array.from(trackTable(b0.value)));
    expect(Array.from(trackTable(b2.value))).not.toEqual(Array.from(trackTable(b1.value)));
    expect(readBlobInfo(b1.value).eventCount).toBe(readBlobInfo(b0.value).eventCount);
  });
});

describe('audition blob', () => {
  it('compiles a one track blob that loads and makes sound for a live note', () => {
    const c = new PreviewCompiler(wasmModule);
    const blob = c.compile('builtin-soft-piano', 48000);
    const info = readBlobInfo(blob);
    expect(info.trackCount).toBe(1);
    expect(info.eventCount).toBe(0);
    const engine = WasmEngine.fromModule(wasmModule);
    engine.load(blob);
    for (const p of previewNotes(60)) engine.note(0, p, 90, true);
    let peak = 0;
    for (let i = 0; i < 40; i++) {
      const [l] = engine.process(128);
      for (const v of l) peak = Math.max(peak, Math.abs(v));
    }
    expect(peak).toBeGreaterThan(0.001);
    engine.free();
    c.dispose();
  });

  it('gives different voices different track tables and never touches a project', () => {
    const c = new PreviewCompiler(wasmModule);
    const a = Array.from(trackTable(c.compile('builtin-warm-pad', 48000)));
    const b = Array.from(trackTable(c.compile('builtin-bell', 48000)));
    expect(a).not.toEqual(b);
    c.dispose();
  });

  it('plays a major triad around the configured root', () => {
    expect(PREVIEW_CHORD).toEqual([0, 4, 7]);
    expect(previewNotes(60)).toEqual([60, 64, 67]);
    expect(previewNotes(126)).toEqual([126, 127, 127]);
  });
});
