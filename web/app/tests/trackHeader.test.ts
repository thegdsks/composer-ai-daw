import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { Store } from '../src/state/store';
import { duplicateTrack, moveTrackTo } from '../src/state/trackActions';
import { clampSlot, copyName, duplicateTrackPlan, reorderTarget, slotAtY, stepTarget } from '../src/state/trackPlans';
import { COLOR_FAMILIES, nextOverride, parseOverrides } from '../src/state/trackView';
import type { Track } from '../src/model';
import { counterIds, newBackend } from './helpers';

describe('reorder index math', () => {
  it('clamps and ignores a move to the same row', () => {
    expect(clampSlot(-3, 4)).toBe(0);
    expect(clampSlot(9, 4)).toBe(3);
    expect(reorderTarget(2, 2, 4)).toBeNull();
    expect(reorderTarget(2, 0, 4)).toBe(0);
    expect(reorderTarget(5, 0, 4)).toBeNull();
  });

  it('finds the row under the pointer from row tops', () => {
    const tops = [100, 164, 228];
    expect(slotAtY(50, tops)).toBe(0);
    expect(slotAtY(170, tops)).toBe(1);
    expect(slotAtY(900, tops)).toBe(2);
  });

  it('steps by one and jumps to the ends, null at the edge', () => {
    expect(stepTarget(1, -1, 3)).toBe(0);
    expect(stepTarget(0, -1, 3)).toBeNull();
    expect(stepTarget(1, 1, 3)).toBe(2);
    expect(stepTarget(2, 1, 3)).toBeNull();
    expect(stepTarget(2, 'start', 3)).toBe(0);
    expect(stepTarget(0, 'end', 3)).toBe(2);
  });
});

describe('duplicate plan', () => {
  const note = { id: 'n1', pitch: 60, velocity: 90, start: 0, duration: 480 };
  const source = {
    id: 'a',
    name: 'Pad',
    kind: 'instrument',
    instrument: 'i1',
    sound: 'builtin-warm-pad',
    mix: { volume: 500, pan: -200, mute: true },
    regions: {
      r2: { id: 'r2', start: 3840, length: 3840, content: { midi: { notes: {} } } },
      r1: { id: 'r1', start: 0, length: 3840, content: { midi: { notes: { n1: note } } } },
    },
  } as unknown as Track;

  it('names the copy and skips taken names', () => {
    expect(copyName('Pad', new Set())).toBe('Pad copy');
    expect(copyName('Pad', new Set(['Pad copy']))).toBe('Pad copy 2');
  });

  it('adds the track after the source with its sound and mix, then copies clips with fresh ids', () => {
    const plan = duplicateTrackPlan([source], source, counterIds());
    const cmds = plan.commands;
    expect(cmds[0]).toMatchObject({ command: 'add_track', name: 'Pad copy', index: 1, kind: 'instrument' });
    expect(cmds.map((c) => c.command)).toEqual([
      'add_track',
      'set_instrument',
      'set_track_sound',
      'set_track_volume',
      'set_track_pan',
      'set_track_mute',
      'duplicate_region',
      'duplicate_region',
    ]);
    const dups = cmds.flatMap((c) => (c.command === 'duplicate_region' ? [c] : []));
    expect(dups.map((c) => c.region)).toEqual(['r1', 'r2']);
    const ids = dups.flatMap((c) => [c.new_region, ...c.note_ids]);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).not.toContain('r1');
    expect(ids).not.toContain('n1');
    expect(dups.every((c) => c.track === plan.track)).toBe(true);
    expect(plan.firstRegion).toBe(dups[0]?.new_region);
  });
});

describe('track color view state', () => {
  it('steps through the families and back to automatic', () => {
    let v = nextOverride(undefined);
    expect(v).toBe(COLOR_FAMILIES[0]);
    for (let i = 1; i < COLOR_FAMILIES.length; i++) v = nextOverride(v);
    expect(v).toBe(COLOR_FAMILIES[COLOR_FAMILIES.length - 1]);
    expect(nextOverride(v)).toBeUndefined();
  });

  it('drops unknown stored values', () => {
    expect(parseOverrides('{"a":"bells","b":"hot pink"}')).toEqual({ a: 'bells' });
    expect(parseOverrides('not json')).toEqual({});
    expect(parseOverrides(null)).toEqual({});
  });
});

describe('track header edits against the engine', () => {
  async function setup(): Promise<Store> {
    const store = new Store(newBackend());
    await store.refresh();
    await store.group('starter', starterCommands(counterIds()));
    return store;
  }

  it('duplicates a track in one undo step labelled Duplicate track', async () => {
    const store = await setup();
    const before = store.getSnapshot();
    const src = before.project.tracks[0]!;
    const newTrack = await duplicateTrack(store, src.id);
    expect(newTrack).not.toBeNull();
    const after = store.getSnapshot();
    expect(after.undoDepth).toBe(before.undoDepth + 1);
    expect(store.stepName(after.undoDepth)).toBe('Duplicate track');
    expect(after.project.tracks).toHaveLength(before.project.tracks.length + 1);
    const copy = after.project.tracks[1]!;
    expect(copy.id).toBe(newTrack);
    expect(Object.keys(copy.regions)).toHaveLength(Object.keys(src.regions).length);
    await store.undo();
    expect(store.getSnapshot().project.tracks).toHaveLength(before.project.tracks.length);
  });

  it('moves a track to the front in one undo step', async () => {
    const store = await setup();
    const before = store.getSnapshot();
    const last = before.project.tracks[before.project.tracks.length - 1]!;
    expect(await moveTrackTo(store, last.id, 0)).toBe(true);
    const after = store.getSnapshot();
    expect(after.project.tracks[0]?.id).toBe(last.id);
    expect(after.undoDepth).toBe(before.undoDepth + 1);
    expect(await moveTrackTo(store, last.id, 0)).toBe(false);
  });
});
