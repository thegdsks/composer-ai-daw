// Builders for the bulk note commands (D22): one command moves, resizes, sets or deletes a whole
// selection, so a gesture over 200 notes is one command and one undo step. Pure.
import type { Command, Id, Note, Region, Tick } from '../model';
import { clampPitch, clampVelocity } from './commands';

export function moveNotesBy(region: Id, notes: readonly Id[], dTick: Tick, dPitch: number): Command[] {
  if (notes.length === 0 || (dTick === 0 && dPitch === 0)) return [];
  return [{ command: 'move_notes', region, notes: [...notes], delta_tick: dTick, delta_pitch: dPitch }];
}

export function resizeNotesBy(region: Id, notes: readonly Id[], delta: Tick): Command[] {
  if (notes.length === 0 || delta === 0) return [];
  return [{ command: 'resize_notes', region, notes: [...notes], delta }];
}

export function deleteNoteIds(region: Id, notes: readonly Id[]): Command[] {
  return notes.length === 0 ? [] : [{ command: 'delete_notes', region, notes: [...notes] }];
}

export function setNotesVelocity(region: Id, notes: readonly Id[], value: number): Command[] {
  if (notes.length === 0) return [];
  return [{ command: 'set_notes_velocity', region, notes: [...notes], value: clampVelocity(value), ramp: null }];
}

export function rampNotesVelocity(region: Id, notes: readonly Id[], from: number, to: number): Command[] {
  if (notes.length === 0) return [];
  const ramp = { from: clampVelocity(from), to: clampVelocity(to) };
  return [{ command: 'set_notes_velocity', region, notes: [...notes], value: null, ramp }];
}

/** Null `notes` means every note of the clip. */
export function quantizeNotes(region: Id, grid: Tick, strength: number, notes: readonly Id[] | null): Command {
  return {
    command: 'quantize_notes',
    region,
    grid,
    strength: Math.min(100, Math.max(1, Math.round(strength))),
    notes: notes ? [...notes] : null,
  };
}

export function transposeNotes(region: Id, semitones: number, notes: readonly Id[] | null): Command {
  return { command: 'transpose_notes', region, semitones, notes: notes ? [...notes] : null };
}

/** Copies of notes shifted by a delta, with new ids (Alt drag). */
export function copyNotesBy(
  region: Region,
  notes: readonly Note[],
  dTick: Tick,
  dPitch: number,
  newId: () => Id,
): Command[] {
  if (notes.length === 0) return [];
  const copies = notes.map((n) => ({
    id: newId(),
    pitch: clampPitch(n.pitch + dPitch),
    velocity: n.velocity,
    start: n.start + dTick,
    duration: n.duration,
  }));
  return [{ command: 'add_notes', region: region.id, notes: copies }];
}
