// Pure helpers for the project key: membership, nearest in-key pitch and the picker lists.
// The interval tables mirror core/composer-theory (D23); a test checks them against the wasm.
import type { ScaleName, TempoMap, Tick } from '../model';

export interface KeyInfo {
  root: number;
  scale: ScaleName;
}

export const SCALES: readonly {
  name: ScaleName;
  label: string;
  intervals: readonly number[];
}[] = [
  { name: 'major', label: 'Major', intervals: [0, 2, 4, 5, 7, 9, 11] },
  { name: 'minor', label: 'Minor', intervals: [0, 2, 3, 5, 7, 8, 10] },
  { name: 'dorian', label: 'Dorian', intervals: [0, 2, 3, 5, 7, 9, 10] },
  {
    name: 'mixolydian',
    label: 'Mixolydian',
    intervals: [0, 2, 4, 5, 7, 9, 10],
  },
  {
    name: 'pentatonic_major',
    label: 'Major pentatonic',
    intervals: [0, 2, 4, 7, 9],
  },
  {
    name: 'pentatonic_minor',
    label: 'Minor pentatonic',
    intervals: [0, 3, 5, 7, 10],
  },
  {
    name: 'harmonic_minor',
    label: 'Harmonic minor',
    intervals: [0, 2, 3, 5, 7, 8, 11],
  },
];

export const ROOT_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'] as const;

const MAX_PITCH = 127;

/** Pitch classes of a key as a lookup indexed by pitch class (0 = C). */
export function keyMask(key: KeyInfo): boolean[] {
  const mask = new Array<boolean>(12).fill(false);
  const scale = SCALES.find((s) => s.name === key.scale);
  if (!scale) return mask;
  for (const i of scale.intervals) mask[(key.root + i) % 12] = true;
  return mask;
}

export function inKey(mask: readonly boolean[], pitch: number): boolean {
  return mask[((pitch % 12) + 12) % 12] === true;
}

/** The key in force at a tick: the last key change at or before it, or null for "Not sure". */
export function keyAt(map: TempoMap, tick: Tick): KeyInfo | null {
  let best = -1;
  let found: KeyInfo | null = null;
  for (const [t, k] of Object.entries(map.keys ?? {})) {
    const at = Number(t);
    if (at <= tick && at > best) {
      best = at;
      found = { root: k.root, scale: k.scale };
    }
  }
  return found;
}

/** Nearest in-key pitch to `pitch` (itself when already in key); a tie goes up. */
export function nearestInKey(mask: readonly boolean[], pitch: number): number {
  if (!mask.some(Boolean)) return pitch;
  for (let d = 0; d <= 12; d++) {
    const up = pitch + d;
    if (up <= MAX_PITCH && inKey(mask, up)) return up;
    const down = pitch - d;
    if (down >= 0 && inKey(mask, down)) return down;
  }
  return pitch;
}

/** The next in-key pitch above (dir 1) or below (dir -1); stays put at the ends of the range. */
export function stepInKey(mask: readonly boolean[], pitch: number, dir: 1 | -1): number {
  for (let p = pitch + dir; p >= 0 && p <= MAX_PITCH; p += dir) {
    if (inKey(mask, p)) return p;
  }
  return pitch;
}

export function keyName(key: KeyInfo): string {
  const scale = SCALES.find((s) => s.name === key.scale);
  return `${ROOT_NAMES[key.root] ?? '?'} ${scale?.label.toLowerCase() ?? key.scale}`;
}
