import { describe, expect, it } from 'vitest';
import type { Note, Region } from '../src/model';
import { addNote, deleteNotes, movedPosition, moveNotes, resizeNote, setTempo } from '../src/state/commands';

const region: Region = { id: 'r', start: 0, length: 3840, content: { midi: { notes: {} } } };
const n = (id: string, start: number, duration: number, pitch = 60): Note => ({
  id,
  start,
  duration,
  pitch,
  velocity: 90,
});

describe('command builders', () => {
  it('clamps pitch and velocity in add_note', () => {
    expect(addNote('i', 'r', 200, 0, 240, 0)).toMatchObject({ command: 'add_note', pitch: 127, velocity: 1 });
  });

  it('keeps moved notes inside the region', () => {
    expect(movedPosition(region, n('a', 0, 960), -500, 0).start).toBe(0);
    expect(movedPosition(region, n('a', 0, 960), 99999, 0).start).toBe(3840 - 960);
    expect(movedPosition(region, n('a', 0, 960), 0, 99).pitch).toBe(127);
  });

  it('emits no command for a move that changes nothing', () => {
    expect(moveNotes(region, [n('a', 0, 960)], 0, 0)).toEqual([]);
    expect(moveNotes(region, [n('a', 0, 960)], 240, 2)).toEqual([
      { command: 'move_note', region: 'r', note: 'a', start: 240, pitch: 62 },
    ]);
  });

  it('limits resize to the region and the minimum', () => {
    const note = n('a', 3000, 240);
    expect(resizeNote(region, note, 99999, 120)).toMatchObject({ duration: 840 });
    expect(resizeNote(region, note, 10, 120)).toMatchObject({ duration: 120 });
    expect(resizeNote(region, note, 240, 120)).toBeNull();
  });

  it('builds deletes and a tempo in thousandths', () => {
    expect(deleteNotes('r', [n('a', 0, 1), n('b', 0, 1)])).toHaveLength(2);
    expect(setTempo(97.5)).toEqual({ command: 'set_tempo', tick: 0, mbpm: 97500 });
    expect(setTempo(5)).toMatchObject({ mbpm: 20000 });
    expect(setTempo(9999)).toMatchObject({ mbpm: 400000 });
  });
});
