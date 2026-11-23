import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { notesOf, type Region, type TempoMap } from '../src/model';
import { barEndAtOrAfter, barStartAt, pasteClipsPlan, pasteNotesPlan, repeatNotesPlan, snapshotNotes } from '../src/state/clipboard';
import { findRegion, Store } from '../src/state/store';
import { counterIds, newActions, newBackend } from './helpers';

const BAR = 3840;
const BEAT = 960;
const map4: TempoMap = { tempos: { '0': 120000 }, meters: { '0': { numerator: 4, denominator: 4 } } };
// 3/4 for two bars (5760 ticks) and then 4/4.
const mapMixed: TempoMap = {
  tempos: { '0': 120000 },
  meters: { '0': { numerator: 3, denominator: 4 }, '5760': { numerator: 4, denominator: 4 } },
};

function clip(start: number, length: number, notes: { id: string; pitch: number; start: number; duration: number }[]): Region {
  const rec: Record<string, { id: string; pitch: number; velocity: number; start: number; duration: number }> = {};
  for (const n of notes) rec[n.id] = { ...n, velocity: 90 };
  return { id: 'r', start, length, content: { midi: { notes: rec } } };
}

const same = (p: number): number => p;

describe('bar math', () => {
  it('finds bar lines across a meter change', () => {
    expect(barStartAt(map4, 3 * BAR + 500)).toBe(3 * BAR);
    expect(barStartAt(mapMixed, 2000)).toBe(0);
    expect(barStartAt(mapMixed, 3000)).toBe(2880);
    expect(barStartAt(mapMixed, 5760 + 100)).toBe(5760);
    expect(barStartAt(mapMixed, 5760 + BAR + 5)).toBe(5760 + BAR);
    expect(barEndAtOrAfter(mapMixed, 3000)).toBe(5760);
    expect(barEndAtOrAfter(map4, BAR)).toBe(BAR);
  });
});

describe('note paste offsets', () => {
  const region = clip(0, 4 * BAR, [
    { id: 'a', pitch: 60, start: BEAT, duration: BEAT },
    { id: 'b', pitch: 64, start: 2 * BEAT, duration: 480 },
  ]);

  it('keeps the beat inside the bar when pasted on another bar line', () => {
    const shapes = snapshotNotes(map4, region, notesOf(region));
    expect(shapes.map((s) => s.offset)).toEqual([BEAT, 2 * BEAT]);
    const plan = pasteNotesPlan(map4, region, shapes, 4 * BAR, counterIds(), same);
    const add = plan.commands.at(-1);
    expect(add?.command).toBe('add_notes');
    if (add?.command !== 'add_notes') return;
    expect(add.notes.map((n) => n.start)).toEqual([4 * BAR + BEAT, 4 * BAR + 2 * BEAT]);
    // The clip grows to the next whole bar to hold them.
    expect(plan.commands[0]).toEqual({ command: 'resize_region', region: 'r', length: 5 * BAR });
  });

  it('measures the origin bar of a copy from a 3/4 bar', () => {
    const three = clip(0, 5760, [{ id: 'a', pitch: 60, start: 3000, duration: 100 }]);
    const shapes = snapshotNotes(mapMixed, three, notesOf(three));
    expect(shapes[0]?.offset).toBe(3000 - 2880);
    const wide = clip(0, 10 * BAR, []);
    const plan = pasteNotesPlan(mapMixed, wide, shapes, 5760 + BAR, counterIds(), same);
    const add = plan.commands.at(-1);
    if (add?.command !== 'add_notes') throw new Error('expected add_notes');
    expect(add.notes[0]?.start).toBe(5760 + BAR + 120);
    expect(plan.grownTo).toBeNull();
  });

  it('gives every pasted note a fresh id and applies Stay in key', () => {
    const shapes = snapshotNotes(map4, region, notesOf(region));
    const plan = pasteNotesPlan(map4, region, shapes, 0, counterIds(), (p) => p + 1);
    const add = plan.commands.at(-1);
    if (add?.command !== 'add_notes') throw new Error('expected add_notes');
    expect(new Set(add.notes.map((n) => n.id)).size).toBe(2);
    expect(add.notes.some((n) => n.id === 'a' || n.id === 'b')).toBe(false);
    expect(add.notes.map((n) => n.pitch)).toEqual([61, 65]);
  });

  it('repeat places each copy one span after the last', () => {
    const plan = repeatNotesPlan(map4, region, notesOf(region), 2, BEAT / 2, counterIds(), same);
    const add = plan.commands.at(-1);
    if (add?.command !== 'add_notes') throw new Error('expected add_notes');
    // The selection spans 960 to 2400 (1440 ticks), so copies start 1440 and 2880 later.
    expect(add.notes.map((n) => n.start)).toEqual([2400, 3360, 3840, 4800]);
    expect(plan.ids).toHaveLength(4);
  });

  it('pastes clips with the first one on the target bar and fresh ids', () => {
    const plan = pasteClipsPlan(
      [
        { track: 't1', offset: 0, length: BAR, notes: [{ pitch: 60, velocity: 90, start: 0, duration: 100 }] },
        { track: 'gone', offset: 2 * BAR, length: BAR, notes: [] },
      ],
      8 * BAR,
      (id) => id === 't1',
      'fallback',
      counterIds(),
    );
    const adds = plan.commands.filter((c) => c.command === 'add_region');
    expect(adds.map((c) => (c.command === 'add_region' ? [c.track, c.start] : []))).toEqual([
      ['t1', 8 * BAR],
      ['fallback', 10 * BAR],
    ]);
    expect(new Set(plan.ids).size).toBe(2);
  });
});

async function setup() {
  const store = new Store(newBackend());
  await store.refresh();
  await store.group('starter', starterCommands(counterIds()));
  const s = await store.backend.state();
  if (s.ok) await store.load(s.value.project);
  await store.refresh();
  const actions = newActions(store);
  const pad = store.getSnapshot().project.tracks[0]!;
  return { store, actions, padClip: Object.values(pad.regions)[0]! };
}

describe('clipboard through the backend', () => {
  it('copies bar 1 notes and pastes them 4 bars later as one undo step', async () => {
    const { store, actions, padClip } = await setup();
    const inBar1 = notesOf(padClip).filter((n) => n.start < BAR);
    expect(inBar1.length).toBeGreaterThan(0);
    expect(actions.copyNotes(padClip.id, new Set(inBar1.map((n) => n.id)))).toBe(true);
    const depth = store.getSnapshot().undoDepth;
    const before = notesOf(findRegion(store.getSnapshot().project, padClip.id)!.region).length;
    expect(await actions.pasteNotes(padClip.id, 4 * BAR)).toBe(true);
    const snap = store.getSnapshot();
    expect(snap.undoDepth).toBe(depth + 1);
    const region = findRegion(snap.project, padClip.id)!.region;
    expect(notesOf(region).length).toBe(before + inBar1.length);
    for (const n of inBar1) {
      expect(notesOf(region).some((m) => m.start === n.start + 4 * BAR && m.pitch === n.pitch && m.id !== n.id)).toBe(true);
    }
    expect(snap.selectedNotes.size).toBe(inBar1.length);
    await store.undo();
    expect(notesOf(findRegion(store.getSnapshot().project, padClip.id)!.region).length).toBe(before);
  });

  it('repeats a clip 4 times as one undo step labelled Repeat 4 times', async () => {
    const { store, actions, padClip } = await setup();
    const depth = store.getSnapshot().undoDepth;
    const clips = Object.keys(store.getSnapshot().project.tracks[0]!.regions).length;
    await actions.duplicateClips([padClip.id], 4);
    const snap = store.getSnapshot();
    expect(snap.undoDepth).toBe(depth + 1);
    expect(store.stepName(snap.undoDepth)).toBe('Repeat 4 times');
    const regions = Object.values(snap.project.tracks[0]!.regions);
    expect(regions.length).toBe(clips + 4);
    expect(regions.map((r) => r.start)).toContain(padClip.start + 4 * padClip.length);
    await store.undo();
    expect(Object.keys(store.getSnapshot().project.tracks[0]!.regions).length).toBe(clips);
  });

  it('cut then paste moves a clip, in two steps', async () => {
    const { store, actions, padClip } = await setup();
    const depth = store.getSnapshot().undoDepth;
    await actions.cutClips([padClip.id]);
    expect(store.getSnapshot().undoDepth).toBe(depth + 1);
    expect(findRegion(store.getSnapshot().project, padClip.id)).toBeNull();
    await actions.pasteInArrangement(8 * BAR);
    const regions = Object.values(store.getSnapshot().project.tracks[0]!.regions);
    expect(regions.some((r) => r.start === 8 * BAR && notesOf(r).length === notesOf(padClip).length)).toBe(true);
  });

  it('says so instead of failing when nothing is copied', async () => {
    const { actions, padClip } = await setup();
    expect(await actions.pasteNotes(padClip.id, 0)).toBe(false);
  });
});
