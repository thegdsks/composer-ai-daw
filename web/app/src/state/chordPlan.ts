// Commands for putting a chord into a clip (U19). Pure: ids come from the caller.
import type { Command, Id, Region, TempoMap, Tick } from '../model';
import { notesOf } from '../model';
import { barAt } from '../time';
import { addNotesPlan, barEndAtOrAfter, barStartAt } from './clipboard';

/** Where a chord goes: the whole bar at the playhead (replacing its notes) or one grid step at the playhead. */
export type ChordMode = 'bar' | 'playhead';

export interface ChordPlan {
  commands: Command[];
  ids: Id[];
  /** Bar of the chord, one based, for the step label. */
  bar: number;
}

/** Raises the lowest note an octave per inversion step. */
export function invert(pitches: readonly number[], steps: number): number[] {
  const out = [...pitches].sort((a, b) => a - b);
  for (let i = 0; i < steps && out.length > 1; i++) {
    const low = out.shift();
    if (low !== undefined) out.push(low + 12);
  }
  return out.map((p) => Math.min(127, p));
}

/** The tick a chord lands at: the playhead, or the clip start when the playhead is before the clip. */
export function chordTick(region: Region, playhead: Tick): Tick {
  return Math.max(region.start, Math.round(playhead));
}

export function chordPlan(
  map: TempoMap,
  region: Region,
  playhead: Tick,
  mode: ChordMode,
  pitches: readonly number[],
  grid: Tick,
  velocity: number,
  newId: () => Id,
): ChordPlan {
  const tick = chordTick(region, playhead);
  const barStart = barStartAt(map, tick);
  const barEnd = barEndAtOrAfter(map, barStart + 1);
  const from = mode === 'bar' ? barStart : tick;
  const length = mode === 'bar' ? barEnd - barStart : Math.max(1, grid);
  const commands: Command[] = [];
  if (mode === 'bar') {
    const gone = notesOf(region)
      .filter((n) => region.start + n.start >= barStart && region.start + n.start < barEnd)
      .map((n) => n.id);
    if (gone.length > 0) commands.push({ command: 'delete_notes', region: region.id, notes: gone });
  }
  const placed = pitches.map((pitch) => ({ pitch, velocity, start: Math.max(0, from - region.start), duration: length }));
  const add = addNotesPlan(map, region, placed, newId, (p) => p);
  commands.push(...add.commands);
  return { commands, ids: add.ids, bar: barAt(map, barStart) + 1 };
}
