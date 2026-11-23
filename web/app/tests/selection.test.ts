import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { notesOf, type Note, type Region } from '../src/model';
import {
  autoGridTicks,
  clampGroupDelta,
  clampGroupResize,
  loudnessName,
  marqueeHits,
  noteSpoken,
  parseBeats,
  parsePitch,
  quantizePreview,
  rampVelocities,
  selectionKeys,
  selectionText,
} from '../src/roll/selection';
import { copyNotesBy, moveNotesBy, quantizeNotes, rampNotesVelocity, setNotesVelocity } from '../src/state/bulkNotes';
import { findRegion, Store } from '../src/state/store';
import { counterIds, newBackend } from './helpers';

const note = (id: string, pitch: number, start: number, duration = 240, velocity = 90): Note => ({ id, pitch, start, duration, velocity });
const region = (length: number, notes: Note[]): Region => ({
  id: 'r',
  start: 0,
  length,
  content: { midi: { notes: Object.fromEntries(notes.map((n) => [n.id, n])) } },
});

describe('marquee', () => {
  const notes = [note('a', 60, 0), note('b', 62, 480), note('c', 64, 960), note('d', 60, 5000), note('e', 72, 960, 2000)].sort(
    (x, y) => x.start - y.start,
  );
  const maxDur = 2000;

  it('selects notes the box touches in time and pitch', () => {
    const hits = marqueeHits(notes, maxDur, 0, { tickA: 400, tickB: 1000, pitchA: 61, pitchB: 65 });
    expect(hits.sort()).toEqual(['b', 'c']);
  });

  it('takes corners in any order and includes a long note that began earlier', () => {
    const hits = marqueeHits(notes, maxDur, 0, { tickA: 2500, tickB: 2000, pitchA: 80, pitchB: 70 });
    expect(hits).toEqual(['e']);
  });

  it('offsets by the clip start and misses everything outside', () => {
    expect(marqueeHits(notes, maxDur, 10000, { tickA: 10400, tickB: 11000, pitchA: 61, pitchB: 65 }).sort()).toEqual(['b', 'c']);
    expect(marqueeHits(notes, maxDur, 0, { tickA: 100, tickB: 200, pitchA: 90, pitchB: 100 })).toEqual([]);
  });
});

describe('velocity ramp', () => {
  it('runs from the first note to the last by start', () => {
    const m = rampVelocities([note('a', 60, 0), note('b', 60, 480), note('c', 60, 960)], 20, 100);
    expect([...m.values()]).toEqual([20, 60, 100]);
  });

  it('gives one value to notes that share a start', () => {
    const m = rampVelocities([note('a', 60, 0), note('b', 64, 0)], 30, 90);
    expect([...m.values()]).toEqual([30, 30]);
  });

  it('matches the core command for a falling ramp', async () => {
    const store = new Store(newBackend('ramp'));
    await store.refresh();
    const ids = counterIds();
    await store.group('Setup', starterCommands(ids));
    const snap = store.getSnapshot();
    const r = snap.project.tracks.flatMap((t) => Object.values(t.regions)).find((x) => notesOf(x).length > 3)!;
    const picked = notesOf(r).sort((a, b) => a.start - b.start);
    const want = rampVelocities(picked, 110, 25);
    await store.group('Ramp', rampNotesVelocity(r.id, picked.map((n) => n.id), 110, 25));
    const after = findRegion(store.getSnapshot().project, r.id)!.region;
    for (const n of notesOf(after)) expect(n.velocity).toBe(want.get(n.id));
  });
});

describe('quantize preview', () => {
  it('equals the core result for 50 random notes at several strengths', async () => {
    for (const [grid, strength] of [[480, 100], [240, 100], [320, 60], [960, 25]] as const) {
      const store = new Store(newBackend(`q${grid}-${strength}`));
      await store.refresh();
      const ids = counterIds();
      const regionId = ids();
      const trackId = ids();
      const length = 8 * 3840;
      let seed = 11;
      const rnd = (n: number): number => {
        seed = (seed * 1103515245 + 12345) % 2 ** 31;
        return seed % n;
      };
      const specs = Array.from({ length: 50 }, () => {
        const duration = 60 + rnd(1500);
        return { id: ids(), pitch: 30 + rnd(60), velocity: 90, start: rnd(length - duration), duration };
      });
      await store.group('Setup', [
        { command: 'add_track', id: trackId, name: 'T', kind: 'instrument', index: null },
        { command: 'add_region', id: regionId, track: trackId, start: 0, length, kind: 'midi' },
        { command: 'add_notes', region: regionId, notes: specs },
      ]);
      const before = findRegion(store.getSnapshot().project, regionId)!.region;
      const preview = quantizePreview(before, notesOf(before), grid, strength);
      const r = await store.group('Quantize', [quantizeNotes(regionId, grid, strength, null)]);
      expect(r.ok).toBe(true);
      const after = findRegion(store.getSnapshot().project, regionId)!.region;
      let moved = 0;
      for (const n of notesOf(after)) {
        expect(n.start).toBe(preview.get(n.id) ?? notesOf(before).find((b) => b.id === n.id)!.start);
        if (preview.has(n.id)) moved++;
      }
      expect(moved).toBeGreaterThan(10);
    }
  });
});

describe('group deltas', () => {
  const r = region(3840, []);
  it('moves a block rigidly inside the clip and the pitch range', () => {
    const g = [note('a', 10, 0, 480), note('b', 120, 3000, 840)];
    expect(clampGroupDelta(r, g, -500, 0)).toEqual({ dTick: 0, dPitch: 0 });
    expect(clampGroupDelta(r, g, 900, 20)).toEqual({ dTick: 0, dPitch: 7 });
    expect(clampGroupDelta(r, g, 0, -50)).toEqual({ dTick: 0, dPitch: -10 });
  });
  it('limits a shared resize', () => {
    const g = [note('a', 60, 0, 480), note('b', 60, 3000, 600)];
    expect(clampGroupResize(r, g, 5000, 120)).toBe(240);
    expect(clampGroupResize(r, g, -5000, 120)).toBe(-360);
  });
});

describe('bulk commands', () => {
  it('emit one command for a whole selection and none for a no-op', () => {
    expect(moveNotesBy('r', ['a', 'b'], 0, 2)).toHaveLength(1);
    expect(moveNotesBy('r', ['a', 'b'], 0, 0)).toEqual([]);
    expect(setNotesVelocity('r', [], 80)).toEqual([]);
  });

  it('copies with new ids and keeps loudness', () => {
    const cmds = copyNotesBy(region(3840, []), [note('a', 60, 0, 240, 70)], 480, 2, counterIds());
    expect(cmds[0]).toMatchObject({ command: 'add_notes', notes: [{ pitch: 62, start: 480, velocity: 70 }] });
  });

  it('moves 200 notes up two semitones as one undo step and undo restores the state', async () => {
    const store = new Store(newBackend('bulk'));
    await store.refresh();
    const ids = counterIds();
    const regionId = ids();
    const trackId = ids();
    const specs = Array.from({ length: 200 }, (_, i) => ({ id: ids(), pitch: 40 + (i % 30), velocity: 90, start: i * 120, duration: 100 }));
    await store.group('Setup', [
      { command: 'add_track', id: trackId, name: 'T', kind: 'instrument', index: null },
      { command: 'add_region', id: regionId, track: trackId, start: 0, length: 200 * 120 + 240, kind: 'midi' },
      { command: 'add_notes', region: regionId, notes: specs },
    ]);
    const hash = store.getSnapshot().stateHash;
    const depth = store.getSnapshot().undoDepth;
    await store.group('Move 200 notes', moveNotesBy(regionId, specs.map((s) => s.id), 0, 2));
    expect(store.getSnapshot().undoDepth).toBe(depth + 1);
    expect(store.getSnapshot().stateHash).not.toBe(hash);
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(hash);
  });
});

describe('words', () => {
  it('names loudness and the keys of a selection', () => {
    expect(loudnessName(30)).toBe('Soft');
    expect(loudnessName(80)).toBe('Medium');
    expect(loudnessName(120)).toBe('Loud');
    expect(selectionKeys([note('a', 64, 0), note('b', 60, 0), note('c', 60, 10)])).toBe('C4, E4');
    expect(selectionText([note('a', 60, 0)])).toBe('1 selected (C4)');
    const many = Array.from({ length: 9 }, (_, i) => note(`n${i}`, 60 + i, 0));
    expect(selectionText(many)).toBe('9 selected (C4, C#4, D4, D#4, E4, F4 and 3 more)');
  });

  it('speaks pitch, bar, beat and length', () => {
    expect(noteSpoken(note('a', 61, 960 + 3840, 480), 0, 3840)).toBe('C#4, bar 2, beat 2, length 0.5 beats');
    expect(noteSpoken(note('a', 60, 0, 960), 3840, 3840)).toBe('C4, bar 2, beat 1, length 1 beat');
  });

  it('parses typed pitches and beats', () => {
    expect(parsePitch('C4')).toBe(60);
    expect(parsePitch('f#3')).toBe(54);
    expect(parsePitch('Bb2')).toBe(46);
    expect(parsePitch('72')).toBe(72);
    expect(parsePitch('H4')).toBeNull();
    expect(parsePitch('200')).toBeNull();
    expect(parseBeats('1.5')).toBe(1440);
    expect(parseBeats('x')).toBeNull();
  });

  it('picks the finest grid that still has wide cells', () => {
    const sizes = [{ ticks: 960 }, { ticks: 480 }, { ticks: 240 }, { ticks: 120 }];
    expect(autoGridTicks(0.1, sizes, 14)).toBe(240);
    expect(autoGridTicks(0.01, sizes, 14)).toBe(960);
    expect(autoGridTicks(1, sizes, 14)).toBe(120);
  });
});
