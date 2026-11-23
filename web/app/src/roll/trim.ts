// Trimming the left edge of a clip keeps every note where it sounds. The clip start and the
// notes' clip relative starts move in opposite directions, so it is one group of commands.
import { notesOf, type Command, type Region, type Tick } from '../model';

/** Furthest the start may move: back to tick 0, or forward to the first note (or the clip end). */
export function trimLimits(region: Region): { min: Tick; max: Tick } {
  let first = region.length - 1;
  for (const n of notesOf(region)) first = Math.min(first, n.start);
  return { min: -region.start, max: Math.max(0, Math.min(first, region.length - 1)) };
}

export function clampTrim(region: Region, delta: Tick): Tick {
  const { min, max } = trimLimits(region);
  return Math.min(Math.max(delta, min), max) || 0;
}

/**
 * Commands that move the clip start by `delta` (positive shrinks it from the left). The order
 * keeps every step valid: grow first when extending, shift notes first when shrinking.
 */
export function trimStartCommands(region: Region, delta: Tick): Command[] {
  const d = clampTrim(region, delta);
  if (d === 0 || !('midi' in region.content)) return [];
  const ids = notesOf(region).map((n) => n.id);
  const shift: Command[] = ids.length > 0 ? [{ command: 'move_notes', region: region.id, notes: ids, delta_tick: -d, delta_pitch: 0 }] : [];
  const move: Command = { command: 'move_region', region: region.id, start: region.start + d };
  const resize: Command = { command: 'resize_region', region: region.id, length: region.length - d };
  return d > 0 ? [...shift, move, resize] : [resize, ...shift, move];
}
