import { describe, expect, it } from 'vitest';
import { renderBlob } from '../src/audio/offline';
import { starterCommands } from '../src/fixtures';
import { mixOf } from '../src/model';
import { panText, volumeText } from '../src/state/mix';
import { Store } from '../src/state/store';
import { liveOnlyBlob, readBlobInfo, trackTable } from '../src/wasm/blob';
import { WasmEngine } from '../src/wasm/engine';
import type { Actions } from '../src/state/actions';
import { counterIds, newActions, newBackend, wasmModule } from './helpers';

async function seeded(): Promise<{ store: Store; actions: Actions; track: string }> {
  const store = new Store(newBackend());
  await store.refresh();
  await store.group('starter', starterCommands(counterIds()));
  const actions = newActions(store);
  return { store, actions, track: store.getSnapshot().project.tracks[0]?.id ?? '' };
}

async function peak(store: Store): Promise<number> {
  const blob = await store.backend.engineBlob(48000);
  if (!blob.ok) throw new Error(blob.error.message);
  return (await renderBlob(wasmModule, blob.value)).peak;
}

describe('track mix commands', () => {
  it('applies volume, pan, mute and solo and undoes each as one step', async () => {
    const { store, actions, track } = await seeded();
    const t = (): ReturnType<typeof mixOf> => mixOf(store.getSnapshot().project.tracks.find((x) => x.id === track)!);
    expect(t()).toEqual({ volume: 1000, pan: 0, mute: false, solo: false });
    const depth = store.getSnapshot().undoDepth;
    await actions.setVolume(track, 500);
    await actions.setPan(track, -300);
    await actions.setMute(track, true);
    await actions.setSolo(track, true);
    expect(t()).toEqual({ volume: 500, pan: -300, mute: true, solo: true });
    expect(store.getSnapshot().undoDepth).toBe(depth + 4);
    await store.undo();
    expect(t().solo).toBe(false);
    await store.undo();
    expect(t().mute).toBe(false);
  });

  it('clamps out of range values before sending', async () => {
    const { store, actions, track } = await seeded();
    await actions.setVolume(track, 99999);
    await actions.setPan(track, -99999);
    const m = mixOf(store.getSnapshot().project.tracks.find((x) => x.id === track)!);
    expect(m.volume).toBe(2000);
    expect(m.pan).toBe(-1000);
  });

  it('rebuilds a different engine blob after each mix command (the app reloads on any hash change)', async () => {
    const { store, actions, track } = await seeded();
    const base = await store.backend.engineBlob(48000);
    if (!base.ok) throw new Error('blob');
    const loud = await peak(store);
    expect(loud).toBeGreaterThan(0.01);
    await actions.setVolume(track, 250);
    const quiet = await store.backend.engineBlob(48000);
    expect(quiet.ok && Array.from(quiet.value)).not.toEqual(Array.from(base.value));
    await actions.setVolume(track, 1000);
    await actions.setPan(track, 1000);
    const panned = await store.backend.engineBlob(48000);
    expect(panned.ok && Array.from(panned.value)).not.toEqual(Array.from(base.value));
    await actions.setPan(track, 0);
    const after = await store.backend.engineBlob(48000);
    expect(after.ok && Array.from(after.value)).toEqual(Array.from(base.value));
  });

  it('mute makes that track silent in the render and solo silences the others', async () => {
    const { store, actions, track } = await seeded();
    const tracks = store.getSnapshot().project.tracks;
    const other = tracks[1]?.id ?? '';
    const all = await peak(store);
    await actions.setMute(track, true);
    await actions.setMute(other, true);
    expect(await peak(store)).toBeLessThan(0.0001);
    await actions.setMute(other, false);
    const onlyOther = await peak(store);
    expect(onlyOther).toBeGreaterThan(0.001);
    expect(onlyOther).toBeLessThanOrEqual(all);
    await actions.setMute(track, false);
    await actions.setSolo(other, true);
    expect(await peak(store)).toBeGreaterThan(0.001);
  });

  it('keeps the worklet blob helpers valid for blob v2 (song and live-only engines both load)', async () => {
    const { store, actions, track } = await seeded();
    await actions.setVolume(track, 700);
    const r = await store.backend.engineBlob(48000);
    if (!r.ok) throw new Error('blob');
    expect(readBlobInfo(r.value).version).toBe(2);
    const song = WasmEngine.fromModule(wasmModule);
    song.load(r.value);
    const live = WasmEngine.fromModule(wasmModule);
    live.load(liveOnlyBlob(r.value));
    expect(readBlobInfo(liveOnlyBlob(r.value)).eventCount).toBe(0);
    expect(trackTable(r.value).length).toBe(2 * 16);
    song.free();
    live.free();
  });

  it('reports measured per track peaks in blob track order, zero for a muted track', async () => {
    const { store, actions } = await seeded();
    const [t0, t1] = store.getSnapshot().project.tracks;
    await actions.setMute(t1!.id, true);
    const r = await store.backend.engineBlob(48000);
    if (!r.ok) throw new Error('blob');
    const eng = WasmEngine.fromModule(wasmModule);
    eng.load(r.value);
    const into = new Float32Array(8);
    let count = 0;
    for (let i = 0; i < 400; i++) {
      eng.process(128);
      count = eng.mergeTrackPeaks(into);
    }
    expect(count).toBe(2);
    expect(t0).toBeDefined();
    expect(into[0]).toBeGreaterThan(0);
    expect(into[1]).toBe(0);
    eng.free();
  });

  it('describes levels in plain words', () => {
    expect(volumeText(1000)).toBe('100 percent');
    expect(volumeText(500)).toBe('50 percent');
    expect(panText(0)).toBe('Center');
    expect(panText(-500)).toBe('Left 50 percent');
    expect(panText(1000)).toBe('Right 100 percent');
  });
});
