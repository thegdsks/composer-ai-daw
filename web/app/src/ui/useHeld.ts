import { useSyncExternalStore } from 'react';
import { isHeld, subscribeHeld } from '../input/heldNotes';

/** Which of these pitches are sounding now (computer keys, MIDI, pointer), as a set that changes only when it must. */
export function useHeld(pitches: readonly number[]): ReadonlySet<number> {
  const key = useSyncExternalStore(subscribeHeld, () => pitches.filter(isHeld).join(','));
  return new Set(key === '' ? [] : key.split(',').map(Number));
}
