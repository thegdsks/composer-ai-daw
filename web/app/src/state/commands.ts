// Builders for the commands the UI sends. Pure: ids are passed in so tests stay deterministic.
import { config } from '../config';
import { notesOf, type Command, type Id, type Note, type Project, type Region, type Tick, type TimeSignature } from '../model';
import { ticksPerBar } from '../time';

const MAX_PITCH = 127;
const MIN_VELOCITY = 1;
const MAX_VELOCITY = 127;

export function clampPitch(p: number): number {
  return Math.min(MAX_PITCH, Math.max(0, Math.round(p)));
}

export function clampVelocity(v: number): number {
  return Math.min(MAX_VELOCITY, Math.max(MIN_VELOCITY, Math.round(v)));
}

export function addNote(
  id: Id,
  region: Id,
  pitch: number,
  start: Tick,
  duration: Tick,
  velocity = config.defaultNoteVelocity,
): Command {
  return {
    command: 'add_note',
    id,
    region,
    pitch: clampPitch(pitch),
    velocity: clampVelocity(velocity),
    start,
    duration,
  };
}

/** Where a note lands after a move, kept inside its region. Used by the drag preview too. */
export function movedPosition(
  region: Region,
  n: Note,
  dTick: Tick,
  dPitch: number,
): { start: Tick; pitch: number } {
  return {
    start: Math.min(Math.max(0, n.start + dTick), Math.max(0, region.length - n.duration)),
    pitch: clampPitch(n.pitch + dPitch),
  };
}

/** Moves notes by a delta, keeping each inside its region; returns no command for no-ops. */
export function moveNotes(
  region: Region,
  notes: readonly Note[],
  dTick: Tick,
  dPitch: number,
): Command[] {
  const out: Command[] = [];
  for (const n of notes) {
    const { start, pitch } = movedPosition(region, n, dTick, dPitch);
    if (start !== n.start || pitch !== n.pitch) {
      out.push({ command: 'move_note', region: region.id, note: n.id, start, pitch });
    }
  }
  return out;
}

/** Duration change clamped so the note stays inside its region and at least one grid step. */
export function resizeNote(region: Region, note: Note, duration: Tick, minDuration: Tick): Command | null {
  const d = Math.min(Math.max(minDuration, duration), region.length - note.start);
  if (d < 1 || d === note.duration) return null;
  return { command: 'resize_note', region: region.id, note: note.id, duration: d };
}

export function deleteNotes(region: Id, notes: readonly Note[]): Command[] {
  return notes.map((n) => ({ command: 'delete_note', region, note: n.id }));
}

export function setTempo(bpm: number): Command {
  const clamped = Math.min(config.tempoMaxBpm, Math.max(config.tempoMinBpm, bpm));
  return { command: 'set_tempo', tick: 0, mbpm: Math.round(clamped * 1000) };
}

/** Default length of a new region: a number of bars of the meter at its start. */
export function defaultRegionLength(sig: TimeSignature): Tick {
  return ticksPerBar(sig) * config.defaultRegionBars;
}

/** Start tick of the bar that contains `tick`, for a single meter map entry at 0. */
export function barStart(tick: Tick, sig: TimeSignature): Tick {
  const bar = ticksPerBar(sig);
  return Math.floor(tick / bar) * bar;
}

/** Notes of a clip in id order, the order the core pairs new ids with. */
function notesById(region: Region): Note[] {
  return notesOf(region).sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/** Cuts a clip at an absolute tick; null when the cut is not strictly inside the clip. */
export function splitClip(region: Region, at: Tick, newId: () => Id): Command | null {
  const cut = at - region.start;
  if (!Number.isInteger(at) || cut <= 0 || cut >= region.length) return null;
  const crossing = notesById(region).filter((n) => n.start < cut && n.start + n.duration > cut);
  return {
    command: 'split_region',
    region: region.id,
    at,
    new_region: newId(),
    note_ids: crossing.map(() => newId()),
  };
}

/** Copy of a clip at `start`, on `track` or on its own track when null. Also the hook for paste and repeat. */
export function duplicateClip(region: Region, start: Tick, track: Id | null, newId: () => Id): Command {
  return {
    command: 'duplicate_region',
    region: region.id,
    new_region: newId(),
    start,
    track,
    note_ids: notesOf(region).map(() => newId()),
  };
}

/** Copies of whole clips right after the block they form, `times` in a row, each on its own track. */
export function repeatClips(regions: readonly Region[], times: number, newId: () => Id): Command[] {
  const from = Math.min(...regions.map((r) => r.start));
  const span = Math.max(...regions.map((r) => r.start + r.length)) - from;
  const out: Command[] = [];
  for (let k = 1; k <= times; k++) for (const r of regions) out.push(duplicateClip(r, r.start + k * span, null, newId));
  return out;
}

/** Commands that put a clip at a start tick on a track; no command for an unchanged field. */
export function placeClip(region: Region, fromTrack: Id, toTrack: Id, start: Tick): Command[] {
  const out: Command[] = [];
  if (fromTrack !== toTrack) out.push({ command: 'move_region_to_track', region: region.id, track: toTrack });
  if (start !== region.start) out.push({ command: 'move_region', region: region.id, start });
  return out;
}

export function clampVolume(v: number): number {
  return Math.min(config.volumeMax, Math.max(0, Math.round(v)));
}

export function clampPan(v: number): number {
  return Math.min(config.panMax, Math.max(-config.panMax, Math.round(v)));
}

export function trackIndex(project: Project, id: Id): number {
  return project.tracks.findIndex((t) => t.id === id);
}
