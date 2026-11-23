import { describe, expect, it } from 'vitest';
import type { Region, TempoMap } from '../src/model';
import { chordPlan, invert } from '../src/state/chordPlan';
import { counterIds } from './helpers';

const BAR = 3840;
const map4: TempoMap = { tempos: { '0': 120000 }, meters: { '0': { numerator: 4, denominator: 4 } } };

function clip(length: number): Region {
  const notes = {
    a: { id: 'a', pitch: 60, velocity: 90, start: 0, duration: 480 },
    b: { id: 'b', pitch: 62, velocity: 90, start: BAR + 480, duration: 480 },
  };
  return { id: 'r', start: 0, length, content: { midi: { notes } } };
}

describe('chord plan', () => {
  it('inverts by raising the lowest note an octave', () => {
    expect(invert([57, 60, 64], 0)).toEqual([57, 60, 64]);
    expect(invert([57, 60, 64], 1)).toEqual([60, 64, 69]);
    expect(invert([57, 60, 64], 2)).toEqual([64, 69, 72]);
  });

  it('replaces the notes of the bar at the playhead', () => {
    const plan = chordPlan(map4, clip(2 * BAR), BAR + 100, 'bar', [57, 60, 64], 480, 100, counterIds());
    expect(plan.bar).toBe(2);
    expect(plan.commands[0]).toMatchObject({ command: 'delete_notes', notes: ['b'] });
    const add = plan.commands[1];
    expect(add).toMatchObject({ command: 'add_notes' });
    if (add?.command === 'add_notes') expect(add.notes.map((n) => [n.pitch, n.start, n.duration])).toEqual([[57, BAR, BAR], [60, BAR, BAR], [64, BAR, BAR]]);
  });

  it('adds one grid step at the playhead and grows the clip when needed', () => {
    const plan = chordPlan(map4, clip(BAR), 2 * BAR, 'playhead', [60, 64], 480, 100, counterIds());
    expect(plan.commands[0]).toMatchObject({ command: 'resize_region', length: 3 * BAR });
    expect(plan.commands).toHaveLength(2);
    expect(plan.commands[1]).toMatchObject({ command: 'add_notes' });
  });
});
