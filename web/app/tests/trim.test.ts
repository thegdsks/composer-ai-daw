import { describe, expect, it } from 'vitest';
import { notesOf } from '../src/model';
import { clampTrim, trimStartCommands } from '../src/roll/trim';
import { findRegion, Store } from '../src/state/store';
import { counterIds, newBackend } from './helpers';

async function setup() {
  const store = new Store(newBackend('trim'));
  await store.refresh();
  const ids = counterIds();
  const region = ids();
  const track = ids();
  await store.group('Setup', [
    { command: 'add_track', id: track, name: 'T', kind: 'instrument', index: null },
    { command: 'add_region', id: region, track, start: 3840, length: 7680, kind: 'midi' },
    {
      command: 'add_notes',
      region,
      notes: [
        { id: ids(), pitch: 60, velocity: 90, start: 960, duration: 480 },
        { id: ids(), pitch: 64, velocity: 90, start: 3000, duration: 480 },
      ],
    },
  ]);
  return { store, region };
}

const absolute = (store: Store, id: string): number[] => {
  const r = findRegion(store.getSnapshot().project, id)!.region;
  return notesOf(r).map((n) => r.start + n.start).sort((a, b) => a - b);
};

describe('left edge trim', () => {
  it('keeps notes where they sound when the clip shrinks from the left, in one undo step', async () => {
    const { store, region } = await setup();
    const before = absolute(store, region);
    const hash = store.getSnapshot().stateHash;
    const depth = store.getSnapshot().undoDepth;
    const r = findRegion(store.getSnapshot().project, region)!.region;
    const res = await store.group('Trim clip start', trimStartCommands(r, 960));
    expect(res.ok).toBe(true);
    const after = findRegion(store.getSnapshot().project, region)!.region;
    expect(after.start).toBe(3840 + 960);
    expect(after.length).toBe(7680 - 960);
    expect(absolute(store, region)).toEqual(before);
    expect(store.getSnapshot().undoDepth).toBe(depth + 1);
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(hash);
  });

  it('keeps notes where they sound when the clip grows to the left', async () => {
    const { store, region } = await setup();
    const before = absolute(store, region);
    const r = findRegion(store.getSnapshot().project, region)!.region;
    const res = await store.group('Trim clip start', trimStartCommands(r, -1920));
    expect(res.ok).toBe(true);
    const after = findRegion(store.getSnapshot().project, region)!.region;
    expect(after.start).toBe(3840 - 1920);
    expect(after.length).toBe(7680 + 1920);
    expect(absolute(store, region)).toEqual(before);
  });

  it('stops at the first note, at tick 0, and does nothing for an audio clip', async () => {
    const { store, region } = await setup();
    const r = findRegion(store.getSnapshot().project, region)!.region;
    expect(clampTrim(r, 99999)).toBe(960);
    expect(clampTrim(r, -99999)).toBe(-3840);
    expect(trimStartCommands({ ...r, content: { audio: { asset: 'a', offset: 0 } } as never }, 100)).toEqual([]);
  });
});
