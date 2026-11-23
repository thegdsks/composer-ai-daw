// Seeded random command generator shared by the mirror and store tests.
import type { Command, Project } from '../src/model';

/** Small seeded generator so a failing run can be replayed. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

type Pick = <T>(xs: T[]) => T | undefined;

/** One random command against what the mirror currently holds; some are invalid on purpose. */
export function randomCommand(p: Project, rand: () => number, id: () => string): Command | Command[] | 'undo' | 'redo' {
  const int = (n: number) => Math.floor(rand() * n);
  const pick: Pick = (xs) => xs[int(xs.length)];
  const track = pick(p.tracks);
  const region = pick(p.tracks.flatMap((t) => Object.values(t.regions)));
  const midi = pick(p.tracks.flatMap((t) => Object.values(t.regions)).filter((r) => 'midi' in r.content));
  const notes = midi && 'midi' in midi.content ? Object.values(midi.content.midi.notes) : [];
  const note = pick(notes);
  const roll = int(32);
  switch (roll) {
    case 0:
      return { command: 'add_track', id: id(), name: `T${int(99)}`, kind: 'instrument', index: null };
    case 1:
      return { command: 'add_track', id: id(), name: `T${int(99)}`, kind: 'instrument', index: int(p.tracks.length + 1) };
    case 2:
      return track ? { command: 'rename_track', track: track.id, name: `R${int(999)}` } : 'undo';
    case 3:
      return track ? { command: 'set_track_volume', track: track.id, volume: int(1500) } : 'undo';
    case 4:
      return track ? { command: 'set_track_pan', track: track.id, pan: int(2001) - 1000 } : 'undo';
    case 5:
      return track ? { command: 'set_track_mute', track: track.id, mute: rand() < 0.5 } : 'undo';
    case 6:
      return track ? { command: 'set_track_solo', track: track.id, solo: rand() < 0.5 } : 'undo';
    case 7:
      return track
        ? { command: 'add_region', id: id(), track: track.id, start: 960 * int(16), length: 3840 * (1 + int(3)), kind: 'midi' }
        : 'redo';
    case 8:
      return region ? { command: 'move_region', region: region.id, start: 960 * int(32) } : 'redo';
    case 9:
      return region ? { command: 'resize_region', region: region.id, length: 960 * (1 + int(20)) } : 'redo';
    case 10:
      return int(4) === 0 && region ? { command: 'delete_region', region: region.id } : 'undo';
    case 11:
    case 12:
    case 13:
    case 14:
      return midi
        ? {
            command: 'add_note',
            id: id(),
            region: midi.id,
            pitch: 40 + int(40),
            velocity: 1 + int(127),
            start: 240 * int(12),
            duration: 120 * (1 + int(8)),
          }
        : 'undo';
    case 15:
      return midi && note ? { command: 'move_note', region: midi.id, note: note.id, start: 240 * int(12), pitch: 40 + int(40) } : 'undo';
    case 16:
      return midi && note ? { command: 'resize_note', region: midi.id, note: note.id, duration: 120 * (1 + int(8)) } : 'undo';
    case 17:
      return midi && note ? { command: 'set_note_velocity', region: midi.id, note: note.id, velocity: 1 + int(127) } : 'undo';
    case 18:
      return midi && note ? { command: 'delete_note', region: midi.id, note: note.id } : 'undo';
    case 19:
      return midi
        ? { command: 'split_region', region: midi.id, at: 960 * (1 + int(3)), new_region: id(), note_ids: [id(), id(), id(), id()] }
        : 'redo';
    case 20:
      return region
        ? { command: 'duplicate_region', region: region.id, new_region: id(), start: 960 * int(40), track: null, note_ids: [id(), id(), id(), id(), id(), id()] }
        : 'redo';
    case 21:
      return region && track ? { command: 'move_region_to_track', region: region.id, track: track.id } : 'redo';
    case 22:
      return int(6) === 0 && track ? { command: 'delete_track', track: track.id } : 'undo';
    case 23:
      return track ? { command: 'move_track', track: track.id, index: int(p.tracks.length) } : 'redo';
    case 24:
      return { command: 'set_tempo', tick: 960 * int(8), mbpm: 60000 + 1000 * int(120) };
    case 25:
      return { command: 'set_time_signature', tick: 3840 * int(4), numerator: 2 + int(6), denominator: 4 };
    case 26:
      return { command: 'set_project_name', name: `Song ${int(99)}` };
    case 27:
      return { command: 'set_key', tick: 3840 * int(3), root: int(12), scale: 'minor' } as unknown as Command;
    case 28:
      return track ? { command: 'set_track_sound', track: track.id, sound: rand() < 0.5 ? null : 'sf2-x' } : 'undo';
    case 29:
      return midi && note
        ? [
            { command: 'move_note', region: midi.id, note: note.id, start: 240 * int(12), pitch: 40 + int(40) },
            { command: 'set_note_velocity', region: midi.id, note: note.id, velocity: 1 + int(127) },
          ]
        : 'undo';
    case 30:
      return 'undo';
    default:
      return 'redo';
  }
}
