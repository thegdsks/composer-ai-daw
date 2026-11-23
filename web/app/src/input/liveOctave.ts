// The octave of the computer keyboard and the on screen keys. View state only, never saved.
import { config } from '../config';

let octave = 0;
const listeners = new Set<() => void>();

export function getLiveOctave(): number {
  return octave;
}

/** Sets the octave inside the configured limits and tells listeners when it changed. */
export function setLiveOctave(next: number): number {
  const clamped = Math.min(config.keyboardOctaveMax, Math.max(config.keyboardOctaveMin, next));
  if (clamped !== octave) {
    octave = clamped;
    for (const l of listeners) l();
  }
  return octave;
}

export function subscribeLiveOctave(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}
