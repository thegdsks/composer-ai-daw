// Pure plans for track header edits (reorder math, duplicate). They return commands; ids are passed in for tests.
import { mixOf, type Command, type Id, type Track } from '../model';
import { duplicateClip } from './commands';

/** Clamps a wanted slot to the list, so the keyboard and the drag never ask move_track for a bad index. */
export function clampSlot(slot: number, count: number): number {
  if (count <= 0) return 0;
  return Math.min(count - 1, Math.max(0, Math.round(slot)));
}

/** Where a track lands after a drag released over `slot` (the row under the pointer); null when nothing changes. */
export function reorderTarget(from: number, slot: number, count: number): number | null {
  if (from < 0 || from >= count) return null;
  const to = clampSlot(slot, count);
  return to === from ? null : to;
}

/** The row under a vertical position, from the tops of the rows (first row first). */
export function slotAtY(y: number, rowTops: readonly number[]): number {
  for (let i = rowTops.length - 1; i >= 0; i--) if (y >= (rowTops[i] ?? 0)) return i;
  return 0;
}

/** Keyboard move target: delta rows, or 'start' and 'end'. */
export function stepTarget(from: number, move: -1 | 1 | 'start' | 'end', count: number): number | null {
  const wanted = move === 'start' ? 0 : move === 'end' ? count - 1 : from + move;
  return reorderTarget(from, wanted, count);
}

/** Name for the copy: "Pad copy", then "Pad copy 2" while taken. */
export function copyName(name: string, taken: ReadonlySet<string>): string {
  const base = `${name} copy`;
  if (!taken.has(base)) return base;
  for (let n = 2; ; n++) if (!taken.has(`${base} ${n}`)) return `${base} ${n}`;
}

/**
 * One group that makes the copy right after the original: a new track with the same sound and mix,
 * then a copy of every clip (with its notes) on it. Existing commands only, new ids everywhere.
 */
export function duplicateTrackPlan(
  tracks: readonly Track[],
  source: Track,
  newId: () => Id,
): { commands: Command[]; track: Id; firstRegion: Id | null } {
  const at = tracks.findIndex((t) => t.id === source.id);
  const id = newId();
  const taken = new Set(tracks.map((t) => t.name));
  const mix = mixOf(source);
  const commands: Command[] = [
    { command: 'add_track', id, name: copyName(source.name, taken), kind: source.kind, index: at < 0 ? null : at + 1 },
  ];
  if (source.instrument !== null) commands.push({ command: 'set_instrument', track: id, instrument: source.instrument });
  if (source.sound) commands.push({ command: 'set_track_sound', track: id, sound: source.sound });
  if (source.mix?.volume !== undefined) commands.push({ command: 'set_track_volume', track: id, volume: mix.volume });
  if (source.mix?.pan !== undefined) commands.push({ command: 'set_track_pan', track: id, pan: mix.pan });
  if (source.reverb_send) commands.push({ command: 'set_track_reverb_send', track: id, send: source.reverb_send });
  if (mix.mute) commands.push({ command: 'set_track_mute', track: id, mute: true });
  if (mix.solo) commands.push({ command: 'set_track_solo', track: id, solo: true });
  let firstRegion: Id | null = null;
  const regions = Object.values(source.regions).sort((a, b) => a.start - b.start || (a.id < b.id ? -1 : 1));
  for (const r of regions) {
    const copy = duplicateClip(r, r.start, id, newId);
    if (copy.command === 'duplicate_region' && firstRegion === null) firstRegion = copy.new_region;
    commands.push(copy);
  }
  return { commands, track: id, firstRegion };
}

/** The undo label of the duplicate group. */
export const DUPLICATE_LABEL = 'Duplicate track';
