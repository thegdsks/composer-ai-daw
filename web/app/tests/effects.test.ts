import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import type { Effect, EffectKind, Project } from '../src/model';
import {
  chainOf,
  clampParam,
  DEVICES,
  deviceOf,
  dropIndex,
  ownerRefusal,
  paramCommand,
  paramText,
  stepIndex,
} from '../src/state/effectsModel';
import { Store } from '../src/state/store';
import { liveOnlyBlob, readBlobInfo } from '../src/wasm/blob';
import { WasmEngine } from '../src/wasm/engine';
import { counterIds, newActions, newBackend, wasmModule } from './helpers';

async function seeded() {
  const store = new Store(newBackend());
  await store.refresh();
  await store.group('starter', starterCommands(counterIds()));
  const actions = newActions(store);
  const track = store.getSnapshot().project.tracks[0]?.id ?? '';
  return { store, actions, track };
}

const fx = (id: string, kind: EffectKind = 'delay'): Effect => ({ id, kind, params: deviceOf(kind).params.map((p) => p.default) });

describe('device table', () => {
  it('lists the six devices with plain names and one line blurbs, no brand words', () => {
    expect(DEVICES.map((d) => d.kind).sort()).toEqual(['crusher', 'delay', 'gate', 'stereo_tool', 'tidal', 'waveshaper']);
    for (const d of DEVICES) {
      expect(d.name.length).toBeGreaterThan(2);
      expect(d.blurb).not.toMatch(/[\u2013\u2014]/);
      expect(d.blurb.length).toBeLessThan(80);
    }
  });

  it('matches the engine: defaults of a fresh device and the ranges in describe()', async () => {
    const { store, actions, track } = await seeded();
    const described = await store.backend.describe();
    if (!described.ok) throw new Error('describe');
    const summary = described.value.find((c) => c.name === 'set_effect_param')?.summary ?? '';
    for (const d of DEVICES) {
      const id = await actions.addEffect({ kind: 'track', track }, d.kind);
      expect(id).not.toBeNull();
      const e = chainOf(store.getSnapshot().project, { kind: 'track', track }).find((x) => x.id === id);
      expect(e?.params).toEqual(d.params.map((p) => p.default));
      const line = summary.split('. ').find((s) => s.startsWith(`${d.kind}: `)) ?? '';
      for (const p of d.params) {
        const range = p.toggle ? `${p.name}` : `${p.name} ${p.min}..${p.max}`;
        expect(line, `${d.kind} ${p.name}`).toContain(range);
      }
    }
  });
});

describe('parameter labels and clamping', () => {
  it('writes values in plain units', () => {
    expect(paramText('delay', 'time_ms', 250)).toBe('250 ms');
    expect(paramText('delay', 'time_ms', 1500)).toBe('1.50 s');
    expect(paramText('delay', 'feedback', 350)).toBe('35%');
    expect(paramText('crusher', 'bits', 8)).toBe('8 bits');
    expect(paramText('crusher', 'rate_reduction', 1)).toBe('None');
    expect(paramText('tidal', 'rate_centihz', 200)).toBe('2.00 Hz');
    expect(paramText('stereo_tool', 'pan', -500)).toBe('50% left');
    expect(paramText('stereo_tool', 'pan', 0)).toBe('Center');
    expect(paramText('stereo_tool', 'mono', 1)).toBe('On');
  });

  it('clamps to the range and rounds', () => {
    expect(clampParam('delay', 'time_ms', 99999)).toBe(2000);
    expect(clampParam('delay', 'time_ms', -5)).toBe(1);
    expect(clampParam('crusher', 'bits', 7.6)).toBe(8);
    expect(clampParam('stereo_tool', 'pan', -5000)).toBe(-1000);
    expect(clampParam('gate', 'attack_ms', Number.NaN)).toBe(0);
    expect(paramText('delay', 'time_ms', 99999)).toBe('2.00 s');
  });

  it('sends nothing when the clamped value is already set', () => {
    const e = fx('a');
    expect(paramCommand(e, 'time_ms', 250)).toBeNull();
    expect(paramCommand(e, 'time_ms', 99999)).toEqual({ command: 'set_effect_param', effect: 'a', param: 'time_ms', value: 2000 });
  });
});

describe('chain plans', () => {
  const chain = [fx('a'), fx('b', 'gate'), fx('c', 'crusher')];
  it('steps one place and stops at the ends', () => {
    expect(stepIndex(3, 0, -1)).toBeNull();
    expect(stepIndex(3, 0, 1)).toBe(1);
    expect(stepIndex(3, 2, 1)).toBeNull();
    expect(stepIndex(3, 2, -1)).toBe(1);
    expect(stepIndex(3, 5, -1)).toBeNull();
  });

  it('drops a card where the target card is', () => {
    expect(dropIndex(chain, 'a', 'c')).toBe(2);
    expect(dropIndex(chain, 'c', 'a')).toBe(0);
    expect(dropIndex(chain, 'b', 'b')).toBeNull();
    expect(dropIndex(chain, 'x', 'a')).toBeNull();
  });

  it('refuses audio tracks and accepts the master', () => {
    const project = { tracks: [{ id: 't', kind: 'audio' }, { id: 'i', kind: 'instrument' }] } as unknown as Project;
    expect(ownerRefusal(project, { kind: 'master' })).toBeNull();
    expect(ownerRefusal(project, { kind: 'track', track: 'i' })).toBeNull();
    expect(ownerRefusal(project, { kind: 'track', track: 't' })).toMatch(/audio/);
  });
});

describe('chain commands through the engine', () => {
  it('adds, changes, bypasses and reorders, each one undo step, and undo restores', async () => {
    const { store, actions, track } = await seeded();
    const owner = { kind: 'track', track } as const;
    const chain = (): readonly Effect[] => chainOf(store.getSnapshot().project, owner);
    const base = store.getSnapshot().undoDepth;

    const echo = await actions.addEffect(owner, 'delay');
    const gate = await actions.addEffect(owner, 'gate');
    expect(chain().map((e) => e.kind)).toEqual(['delay', 'gate']);
    await actions.setEffectParam(chain()[0]!, 'time_ms', 500);
    expect(chain()[0]?.params[0]).toBe(500);
    await actions.setEffectBypass(echo!, true);
    expect(chain()[0]?.bypass).toBe(true);
    await actions.moveEffect(gate!, 0);
    expect(chain().map((e) => e.kind)).toEqual(['gate', 'delay']);
    expect(store.getSnapshot().undoDepth).toBe(base + 5);

    await store.undo();
    expect(chain().map((e) => e.kind)).toEqual(['delay', 'gate']);
    await store.undo();
    expect(chain()[0]?.bypass).toBeFalsy();
    await store.undo();
    expect(chain()[0]?.params[0]).toBe(250);
    await store.undo();
    await store.undo();
    expect(chain()).toHaveLength(0);
  });

  it('runs a master chain and refuses a value outside the range', async () => {
    const { store, actions } = await seeded();
    const id = await actions.addEffect({ kind: 'master' }, 'waveshaper');
    expect(id).not.toBeNull();
    expect(chainOf(store.getSnapshot().project, { kind: 'master' })).toHaveLength(1);
    const e = chainOf(store.getSnapshot().project, { kind: 'master' })[0]!;
    const r = await store.apply({ command: 'set_effect_param', effect: e.id, param: 'drive', value: 5000 });
    expect(r.ok).toBe(false);
  });
});

describe('engine blob with effects', () => {
  it('is blob v7 and both the song and the live only engines load it', async () => {
    const { store, actions, track } = await seeded();
    await actions.addEffect({ kind: 'track', track }, 'delay');
    await actions.addEffect({ kind: 'master' }, 'gate');
    const r = await store.backend.engineBlob(48000);
    if (!r.ok) throw new Error('blob');
    expect(readBlobInfo(r.value).version).toBe(7);
    const live = liveOnlyBlob(r.value);
    expect(readBlobInfo(live).eventCount).toBe(0);
    const song = WasmEngine.fromModule(wasmModule);
    song.load(r.value);
    const liveEngine = WasmEngine.fromModule(wasmModule);
    liveEngine.load(live);
    expect(song.loaded && liveEngine.loaded).toBe(true);
    song.free();
    liveEngine.free();
  });
});
