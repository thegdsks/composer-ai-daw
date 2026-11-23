// U36: the rolling buffer with a fake clock, the buffer to clip plan, and commit as one undo step.
import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { CatchBuffer, catchPlan, catchTakes, freeClipStart } from '../src/input/catchBuffer';
import type { TempoMap } from '../src/model';
import { CatchActions } from '../src/state/catchActions';
import { findRegion, sortedNotes, Store } from '../src/state/store';
import { counterIds, newBackend } from './helpers';

const BAR = 3840;
const QUARTER = 960;
const mapAt = (bpm: number): TempoMap => ({ tempos: { '0': bpm * 1000 }, meters: { '0': { numerator: 4, denominator: 4 } } });

describe('CatchBuffer', () => {
  it('keeps only the last window of notes', () => {
    const b = new CatchBuffer({ windowMs: 10_000, maxNotes: 100 });
    b.noteOn(60, 90, 0, 't');
    b.noteOff(60, 500);
    b.noteOn(62, 90, 8_000, 't');
    b.noteOff(62, 8_500);
    expect(b.count(9_000)).toBe(2);
    // At 10.6 s the first note ended 10.1 s ago and drops out.
    expect(b.notes(10_600).map((n) => n.pitch)).toEqual([62]);
    expect(b.count(30_000)).toBe(0);
  });

  it('keeps a note that is still held and closes it at now', () => {
    const b = new CatchBuffer({ windowMs: 10_000, maxNotes: 100 });
    b.noteOn(60, 90, 1_000, null);
    const [n] = b.notes(4_000);
    expect(n?.end).toBe(4_000);
    expect(b.count(60_000)).toBe(1);
  });

  it('caps the count by dropping the oldest', () => {
    const b = new CatchBuffer({ windowMs: 60_000, maxNotes: 3 });
    for (let i = 0; i < 5; i++) {
      b.noteOn(60 + i, 90, i * 100, null);
      b.noteOff(60 + i, i * 100 + 50);
    }
    expect(b.notes(1_000).map((n) => n.pitch)).toEqual([62, 63, 64]);
  });

  it('closes a repeated press of the same pitch', () => {
    const b = new CatchBuffer({ windowMs: 60_000, maxNotes: 10 });
    b.noteOn(60, 90, 0, null);
    b.noteOn(60, 90, 300, null);
    b.noteOff(60, 600);
    expect(b.notes(700).map((n) => [n.start, n.end])).toEqual([
      [0, 300],
      [300, 600],
    ]);
  });
});

describe('buffer to clip plan', () => {
  const played = [0, 500, 1000, 1500].map((ms, i) => ({ pitch: 60 + i, velocity: 100, start: ms, end: ms + 250, track: 't' }));

  it('puts the first note at the clip start and times ticks by tempo', () => {
    // 120 bpm: 500 ms is one quarter note.
    const takes = catchTakes(played, mapAt(120), 0);
    expect(takes.map((t) => Math.round(t.start))).toEqual([0, QUARTER, 2 * QUARTER, 3 * QUARTER]);
    expect(Math.round(takes[0]?.duration ?? 0)).toBe(QUARTER / 2);
    // Half the tempo, twice the ticks apart in ms terms: 60 bpm makes 500 ms half a quarter.
    expect(Math.round(catchTakes(played, mapAt(60), 0)[1]?.start ?? 0)).toBe(QUARTER / 2);
  });

  it('is add_region then one add_notes, clip length whole bars', () => {
    const ids = counterIds();
    const cmds = catchPlan(played, { map: mapAt(120), track: 'trk', region: 'reg', clipStart: BAR, bar: BAR, grid: 480, newId: ids });
    expect(cmds.map((c) => c.command)).toEqual(['add_region', 'add_notes']);
    expect(cmds[0]).toMatchObject({ id: 'reg', track: 'trk', start: BAR, length: BAR, kind: 'midi' });
    const add = cmds[1];
    expect(add?.command === 'add_notes' && add.notes.map((n) => n.start)).toEqual([0, 960, 1920, 2880]);
  });

  it('grows the clip when the take is longer than a bar', () => {
    const long = [0, 3000].map((ms, i) => ({ pitch: 60 + i, velocity: 100, start: ms, end: ms + 250, track: null }));
    const cmds = catchPlan(long, { map: mapAt(120), track: 't', region: 'r', clipStart: 0, bar: BAR, grid: null });
    expect(cmds.map((c) => c.command)).toEqual(['add_region', 'resize_region', 'add_notes']);
    expect(cmds[1]).toMatchObject({ length: 2 * BAR });
  });

  it('plans nothing for an empty buffer and skips taken bars', () => {
    expect(catchPlan([], { map: mapAt(120), track: 't', region: 'r', clipStart: 0, bar: BAR, grid: null })).toEqual([]);
    expect(freeClipStart([{ start: 0, length: 2 * BAR }], 0, BAR, BAR)).toBe(2 * BAR);
    expect(freeClipStart([{ start: 0, length: BAR }], BAR, BAR, BAR)).toBe(BAR);
  });
});

describe('commit', () => {
  it('writes one clip in one undo step and empties the buffer', async () => {
    const store = new Store(newBackend());
    await store.refresh();
    await store.group('starter', starterCommands(counterIds()));
    let now = 1_000;
    const notices: string[] = [];
    const catcher = new CatchActions(
      store,
      { now: () => now, playTick: () => 0, grid: () => null, limits: { windowMs: 30_000, maxNotes: 100 } },
      (m) => notices.push(m),
    );
    expect(await catcher.commit()).toBe(0);
    expect(notices).toHaveLength(1);

    store.selectRegion(store.getSnapshot().project.tracks[0]?.id ?? '', null);
    for (let i = 0; i < 4; i++) {
      catcher.noteOn(60 + i, 100);
      now += 200;
      catcher.noteOff(60 + i);
      now += 100;
    }
    expect(catcher.getSnapshot().count).toBe(4);
    const before = store.getSnapshot();
    const regionsBefore = before.project.tracks.reduce((n, t) => n + Object.keys(t.regions).length, 0);

    expect(await catcher.commit()).toBe(4);
    const after = store.getSnapshot();
    expect(after.undoDepth).toBe(before.undoDepth + 1);
    expect(after.project.tracks.reduce((n, t) => n + Object.keys(t.regions).length, 0)).toBe(regionsBefore + 1);
    const found = findRegion(after.project, after.selectedRegion);
    expect(found && sortedNotes(found.region).map((n) => n.pitch)).toEqual([60, 61, 62, 63]);
    expect(catcher.getSnapshot().count).toBe(0);

    await store.undo();
    const undone = store.getSnapshot();
    expect(undone.project.tracks.reduce((n, t) => n + Object.keys(t.regions).length, 0)).toBe(regionsBefore);
    catcher.dispose();
  });
});
