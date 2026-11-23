import { describe, expect, it } from 'vitest';
import { gridChoices, gridTicks } from '../src/config';
import type { TempoMap } from '../src/model';
import {
  barLines,
  bpmAt,
  isBlackKey,
  meterSegments,
  pitchName,
  secondsToTicks,
  snapFloor,
  snapRound,
  ticksPerBar,
  ticksToSeconds,
} from '../src/time';

const map = (tempos: Record<string, number>, meters: TempoMap['meters']): TempoMap => ({ tempos, meters });
const four = { numerator: 4, denominator: 4 };

describe('grid', () => {
  it('derives plain and triplet steps from the denominators', () => {
    expect(gridTicks(4, false)).toBe(960);
    expect(gridTicks(16, false)).toBe(240);
    expect(gridTicks(32, false)).toBe(120);
    expect(gridTicks(8, true)).toBe(320);
    expect(gridTicks(4, true)).toBe(640);
  });

  it('offers 1/4 to 1/32 with triplets', () => {
    const ids = gridChoices().map((c) => c.id);
    expect(ids).toEqual(['1/4', '1/4t', '1/8', '1/8t', '1/16', '1/16t', '1/32', '1/32t']);
  });

  it('snaps to the nearest and to the floor', () => {
    expect(snapRound(359, 240)).toBe(240);
    expect(snapRound(361, 240)).toBe(480);
    expect(snapFloor(479, 240)).toBe(240);
  });
});

describe('bars', () => {
  it('sizes bars from the meter', () => {
    expect(ticksPerBar(four)).toBe(3840);
    expect(ticksPerBar({ numerator: 3, denominator: 4 })).toBe(2880);
    expect(ticksPerBar({ numerator: 6, denominator: 8 })).toBe(2880);
  });

  it('lists bar lines and beat lines in a range', () => {
    const m = map({ '0': 120000 }, { '0': four });
    const lines = barLines(m, 0, 7680, true);
    expect(lines.filter((l) => l.isBar).map((l) => [l.tick, l.bar])).toEqual([
      [0, 0],
      [3840, 1],
    ]);
    expect(lines.filter((l) => !l.isBar).map((l) => l.tick)).toEqual([960, 1920, 2880, 4800, 5760, 6720]);
  });

  it('keeps bar numbers across a meter change', () => {
    const m = map({ '0': 120000 }, { '0': four, '7680': { numerator: 3, denominator: 4 } });
    expect(meterSegments(m).map((s) => s.firstBar)).toEqual([0, 2]);
    const bars = barLines(m, 0, 7680 + 2880 * 2, false).map((l) => [l.tick, l.bar]);
    expect(bars).toEqual([
      [0, 0],
      [3840, 1],
      [7680, 2],
      [10560, 3],
    ]);
  });

  it('starts mid range without losing bar numbers', () => {
    const m = map({ '0': 120000 }, { '0': four });
    expect(barLines(m, 5000, 12000, false).map((l) => l.bar)).toEqual([2, 3]);
  });
});

describe('tempo', () => {
  it('converts ticks to seconds at a fixed tempo', () => {
    const m = map({ '0': 120000 }, { '0': four });
    expect(ticksToSeconds(m, 960)).toBeCloseTo(0.5, 9);
    expect(secondsToTicks(m, 2)).toBeCloseTo(3840, 6);
  });

  it('follows a tempo change and inverts it', () => {
    const m = map({ '0': 120000, '3840': 60000 }, { '0': four });
    expect(ticksToSeconds(m, 3840)).toBeCloseTo(2, 9);
    expect(ticksToSeconds(m, 3840 + 960)).toBeCloseTo(3, 9);
    for (const t of [0, 100, 3840, 5000, 20000]) {
      expect(secondsToTicks(m, ticksToSeconds(m, t))).toBeCloseTo(t, 6);
    }
    expect(bpmAt(m, 4000)).toBe(60);
    expect(bpmAt(m, 10)).toBe(120);
  });
});

describe('pitch', () => {
  it('names notes and finds black keys', () => {
    expect(pitchName(60)).toBe('C4');
    expect(pitchName(69)).toBe('A4');
    expect(isBlackKey(61)).toBe(true);
    expect(isBlackKey(62)).toBe(false);
  });
});
