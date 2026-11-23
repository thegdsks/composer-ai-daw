// Pitches that are sounding right now from the computer keyboard, MIDI or the roll keys.
// The piano roll draws them on its keyboard column. View state only, never saved.
const held = new Map<number, number>();
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}

export function holdNote(pitch: number): void {
  held.set(pitch, (held.get(pitch) ?? 0) + 1);
  emit();
}

export function releaseNote(pitch: number): void {
  const n = held.get(pitch);
  if (n === undefined) return;
  if (n <= 1) held.delete(pitch);
  else held.set(pitch, n - 1);
  emit();
}

export function isHeld(pitch: number): boolean {
  return held.has(pitch);
}

export function subscribeHeld(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}
