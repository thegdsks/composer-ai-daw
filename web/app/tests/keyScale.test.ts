import { describe, expect, it } from 'vitest';
import { inKey, keyAt, keyMask, nearestInKey, stepInKey } from '../src/roll/keyScale';
import { newBackend } from './helpers';

const aMinor = keyMask({ root: 9, scale: 'minor' });

describe('key membership', () => {
  it('marks the natural notes in A minor and not the sharps', () => {
    for (const p of [57, 59, 60, 62, 64, 65, 67]) expect(inKey(aMinor, p)).toBe(true);
    for (const p of [58, 61, 63, 66, 68]) expect(inKey(aMinor, p)).toBe(false);
  });

  it('works in every octave', () => {
    expect(inKey(aMinor, 9)).toBe(true);
    expect(inKey(aMinor, 117)).toBe(true);
  });
});

describe('nearest in key pitch', () => {
  it('keeps an in key pitch', () => {
    expect(nearestInKey(aMinor, 69)).toBe(69);
  });

  it('moves G sharp to A (up on a tie with G)', () => {
    expect(nearestInKey(aMinor, 68)).toBe(69);
  });

  it('picks the closer side', () => {
    const cMajor = keyMask({ root: 0, scale: 'major' });
    expect(nearestInKey(cMajor, 61)).toBe(62);
    expect(nearestInKey(cMajor, 70)).toBe(71);
    const pent = keyMask({ root: 0, scale: 'pentatonic_major' });
    expect(nearestInKey(pent, 65)).toBe(64);
  });

  it('clamps at the ends of the range', () => {
    expect(nearestInKey(keyMask({ root: 2, scale: 'major' }), 0)).toBe(1);
    expect(nearestInKey(keyMask({ root: 7, scale: 'major' }), 127)).toBe(127);
  });

  it('is a no-op for an empty mask', () => {
    expect(nearestInKey(new Array<boolean>(12).fill(false), 61)).toBe(61);
  });

  it('steps to the next key pitch in each direction', () => {
    expect(stepInKey(aMinor, 69, 1)).toBe(71);
    expect(stepInKey(aMinor, 71, 1)).toBe(72);
    expect(stepInKey(aMinor, 72, -1)).toBe(71);
    expect(stepInKey(aMinor, 127, 1)).toBe(127);
  });
});

describe('project key from the wasm backend', () => {
  it('set_key shows up in the tempo map and keyAt reads it', async () => {
    const b = newBackend('key-test');
    const r = await b.apply({ command: 'set_key', tick: 0, root: 9, scale: 'minor' });
    expect(r.ok).toBe(true);
    const st = await b.state();
    if (!st.ok) throw new Error('state failed');
    const map = st.value.project.tempo_map;
    expect(keyAt(map, 0)).toEqual({ root: 9, scale: 'minor' });
    expect(keyAt(map, 99999)).toEqual({ root: 9, scale: 'minor' });
    await b.apply({ command: 'remove_key', tick: 0 });
    const after = await b.state();
    if (!after.ok) throw new Error('state failed');
    expect(keyAt(after.value.project.tempo_map, 0)).toBeNull();
  });
});
