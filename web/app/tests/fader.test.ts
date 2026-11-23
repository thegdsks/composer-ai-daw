import { describe, expect, it } from 'vitest';
import { config } from '../src/config';
import { clampDb, dbFraction, dbText, dbToPermille, faderMaxDb, fractionToDb, permilleToDb } from '../src/state/fader';
import { heldLevel, isAudible, litSegments, trackLevel, trackMeterLevel } from '../src/state/meter';
import type { Project, Track } from '../src/model';

describe('fader dB mapping', () => {
  it('round trips every permille within 1', () => {
    for (let p = 1; p <= config.volumeMax; p++) {
      expect(Math.abs(dbToPermille(permilleToDb(p)) - p)).toBeLessThanOrEqual(1);
    }
  });
  it('maps unity to 0 dB and zero to the floor', () => {
    expect(permilleToDb(config.volumeUnity)).toBeCloseTo(0, 9);
    expect(dbToPermille(0)).toBe(config.volumeUnity);
    expect(permilleToDb(0)).toBe(config.faderMinDb);
    expect(dbToPermille(config.faderMinDb)).toBe(0);
  });
  it('tops out at the volume maximum and never exceeds it', () => {
    expect(dbToPermille(faderMaxDb())).toBe(config.volumeMax);
    expect(dbToPermille(faderMaxDb() + 20)).toBe(config.volumeMax);
    expect(clampDb(-1000)).toBe(config.faderMinDb);
  });
  it('maps fractions to dB and back', () => {
    expect(fractionToDb(0)).toBe(config.faderMinDb);
    expect(dbFraction(faderMaxDb())).toBeCloseTo(1, 9);
    expect(dbFraction(fractionToDb(0.4))).toBeCloseTo(0.4, 9);
  });
  it('words the readout', () => {
    expect(dbText(config.faderMinDb)).toBe('Silent');
    expect(dbText(0)).toBe('0.0 dB');
    expect(dbText(3.04)).toBe('+3.0 dB');
  });
});

function track(id: string, mix: Track['mix'], velocity = 100): Track {
  return {
    id,
    name: id,
    kind: 'instrument',
    instrument: null,
    mix,
    regions: {
      r: { id: 'r', start: 0, length: 1000, content: { midi: { notes: { n: { id: 'n', pitch: 60, velocity, start: 0, duration: 500 } } } } },
    },
  };
}
const project = (tracks: Track[]): Project => ({
  id: 'p',
  name: 'p',
  tempo_map: { tempos: { '0': 120000 }, meters: { '0': { numerator: 4, denominator: 4 } } },
  tracks,
});

describe('track meter estimate', () => {
  it('reads zero for a muted track and when another track is soloed', () => {
    const a = track('a', { mute: true });
    const b = track('b', { solo: true });
    const c = track('c', undefined);
    const p = project([a, b, c]);
    expect(trackLevel(p, a, 100, true)).toBe(0);
    expect(trackLevel(p, c, 100, true)).toBe(0);
    expect(trackLevel(p, b, 100, true)).toBeGreaterThan(0);
    expect(isAudible(p, b)).toBe(true);
  });
  it('is silent when stopped or between notes, and follows volume', () => {
    const t = track('t', undefined);
    const p = project([t]);
    expect(trackLevel(p, t, 100, false)).toBe(0);
    expect(heldLevel(t, 700)).toBe(0);
    const quiet = track('q', { volume: 100 });
    expect(trackLevel(p, quiet, 100, true)).toBeLessThan(trackLevel(p, t, 100, true));
  });
  it('lights no segments for zero and all for full scale', () => {
    expect(litSegments(0)).toBe(0);
    expect(litSegments(1)).toBe(config.meterSegments);
    expect(litSegments(0.01)).toBeGreaterThan(0);
  });
});

describe('measured track meter', () => {
  const p = project([track('a', undefined), track('b', undefined)]);
  const [a, b] = p.tracks as [Track, Track];
  it('uses the engine peaks when they cover every track, even while stopped', () => {
    const peaks = new Float32Array([0.25, 0]);
    expect(trackMeterLevel(p, a, 100, false, peaks)).toBe(0.25);
    expect(trackMeterLevel(p, b, 100, true, peaks)).toBe(0);
  });
  it('caps at full scale', () => {
    expect(trackMeterLevel(p, a, 100, true, new Float32Array([2, 0]))).toBe(1);
  });
  it('falls back to the estimate when peaks are missing or stale', () => {
    expect(trackMeterLevel(p, a, 100, true, new Float32Array(0))).toBe(trackLevel(p, a, 100, true));
    expect(trackMeterLevel(p, a, 100, false, new Float32Array(1))).toBe(0);
    expect(trackMeterLevel(p, a, 100, true, new Float32Array(3))).toBe(trackLevel(p, a, 100, true));
  });
});
