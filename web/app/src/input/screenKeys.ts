// Layout and chord pad math for the on screen keyboard. Pure.
import { config } from '../config';
import { isBlackKey } from '../time';
import type { TheoryChord } from '../backend/theory';

/** One white key and the black key that sits just after it (null when there is none). */
export interface KeyGroup {
  white: number;
  black: number | null;
}

/** Keys from the C that the computer keys start on at `octave`, `octaves` octaves and the C above. */
export function keyGroups(octave: number, octaves: number): KeyGroup[] {
  const low = config.keyboardBaseNote + octave * 12;
  const high = low + octaves * 12;
  const out: KeyGroup[] = [];
  for (let p = low; p <= high; p++) {
    if (p < 0 || p > 127 || isBlackKey(p)) continue;
    const next = p + 1;
    out.push({ white: p, black: p < high && next <= 127 && isBlackKey(next) ? next : null });
  }
  return out;
}

export interface ChordPad {
  number: number;
  name: string;
  roman: string;
  pitches: number[];
}

/**
 * Pads in degree order, numbered from 1. The tables place every chord in one octave of C, so a chord
 * whose root lies below the key's root moves up an octave and the pads climb from the tonic.
 */
export function chordPads(chords: readonly TheoryChord[], max: number, keyRoot: number): ChordPad[] {
  return chords.slice(0, max).map((c, i) => {
    const lift = (c.pitches[0] ?? 0) % 12 < keyRoot ? 12 : 0;
    return { number: i + 1, name: c.name, roman: c.roman, pitches: c.pitches.map((p) => p + lift).filter((p) => p >= 0 && p <= 127) };
  });
}
