// U36: key to pitch mapping with octave shift, the on screen layout, chord pads in A minor and
// the number keys. The octave store is module state, so each test resets it.
import { afterEach, describe, expect, it } from 'vitest';
import { keyToPitch } from '../src/input/liveInput';
import { chordPadIndex, matchShortcut } from '../src/input/keymap';
import { getLiveOctave, setLiveOctave } from '../src/input/liveOctave';
import { chordPads, keyGroups } from '../src/input/screenKeys';
import { newBackend } from './helpers';

afterEach(() => setLiveOctave(0));

const key = (init: Partial<KeyboardEvent> & { code: string }): KeyboardEvent => ({ key: '', ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...init }) as KeyboardEvent;

describe('octave shift and key to pitch', () => {
  it('moves the computer keys by twelve per octave and clamps at the limits', () => {
    expect(keyToPitch('KeyA', getLiveOctave())).toBe(60);
    setLiveOctave(1);
    expect(keyToPitch('KeyA', getLiveOctave())).toBe(72);
    expect(keyToPitch('KeyS', getLiveOctave())).toBe(74);
    expect(setLiveOctave(99)).toBe(3);
    expect(setLiveOctave(-99)).toBe(-3);
    expect(keyToPitch('KeyA', -3)).toBe(24);
  });

  it('lays out two octaves plus the top C starting at the same C as the A key', () => {
    const groups = keyGroups(0, 2);
    expect(groups).toHaveLength(15);
    expect(groups[0]).toEqual({ white: 60, black: 61 });
    expect(groups[2]).toEqual({ white: 64, black: null });
    expect(groups[14]).toEqual({ white: 84, black: null });
    expect(groups.flatMap((g) => (g.black === null ? [] : [g.black]))).toHaveLength(10);
    expect(keyGroups(1, 2)[0]?.white).toBe(72);
  });

  it('stays inside the MIDI range at the top', () => {
    const top = keyGroups(3, 2).flatMap((g) => (g.black === null ? [g.white] : [g.white, g.black]));
    expect(Math.max(...top)).toBeLessThanOrEqual(127);
  });
});

describe('chord pads', () => {
  it('gives the triads of A minor from the key tables, numbered from 1', async () => {
    const r = await newBackend().theoryChords?.({ root: 9, scale: 'minor' });
    expect(r?.ok).toBe(true);
    if (!r?.ok) return;
    const pads = chordPads(r.value.triads, 7, 9);
    expect(pads.map((p) => p.name)).toEqual(['Am', 'Bdim', 'C', 'Dm', 'Em', 'F', 'G']);
    expect(pads.map((p) => p.number)).toEqual([1, 2, 3, 4, 5, 6, 7]);
    expect(pads[0]?.pitches).toEqual([69, 72, 76]);
    expect(pads[2]?.pitches).toEqual([72, 76, 79]);
    expect(pads[6]?.pitches).toEqual([79, 83, 86]);
    expect(chordPads(r.value.sevenths, 7, 9)[0]?.pitches).toEqual([69, 72, 76, 79]);
    expect(chordPads(r.value.triads, 3, 9)).toHaveLength(3);
  });
});

describe('shortcuts', () => {
  it('maps plain digits to pads and nothing else', () => {
    expect(chordPadIndex(key({ code: 'Digit1' }))).toBe(0);
    expect(chordPadIndex(key({ code: 'Digit7' }))).toBe(6);
    expect(chordPadIndex(key({ code: 'Digit1', ctrlKey: true }))).toBeNull();
    expect(chordPadIndex(key({ code: 'KeyA' }))).toBeNull();
  });

  it('Cmd or Ctrl Shift M is Catch', () => {
    expect(matchShortcut(key({ code: 'KeyM', key: 'M', metaKey: true, shiftKey: true }))).toBe('catch');
    expect(matchShortcut(key({ code: 'KeyM', key: 'M', ctrlKey: true, shiftKey: true }))).toBe('catch');
    expect(matchShortcut(key({ code: 'KeyM', key: 'm' }))).toBe('metronome');
  });
});
