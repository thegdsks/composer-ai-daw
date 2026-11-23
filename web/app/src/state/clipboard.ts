// Client clipboard and the pure builders behind copy, paste, duplicate and repeat.
// Everything here returns commands; ids are passed in so tests stay deterministic.
import { config } from '../config';
import { isMidi, notesOf, type Command, type Id, type Note, type Region, type TempoMap, type Tick } from '../model';
import { meterSegments, ticksPerBar } from '../time';

/** A copied note: ticks are relative to the start of the bar the first copied note sits in. */
export interface NoteShape {
  pitch: number;
  velocity: number;
  offset: Tick;
  duration: Tick;
}

/** A copied clip with its notes; the offset is relative to the earliest copied clip. */
export interface ClipShape {
  track: Id;
  offset: Tick;
  length: Tick;
  notes: { pitch: number; velocity: number; start: Tick; duration: Tick }[];
}

export type ClipboardContent =
  | { kind: 'notes'; notes: NoteShape[] }
  | { kind: 'clips'; clips: ClipShape[] };

export class Clipboard {
  private content: ClipboardContent | null = null;

  set(content: ClipboardContent): void {
    this.content = content;
  }

  get(): ClipboardContent | null {
    return this.content;
  }
}

export function wordCount(n: number, one: string, many = `${one}s`): string {
  return n === 1 ? `1 ${one}` : `${n} ${many}`;
}

function segmentAt(map: TempoMap, tick: Tick): { tick: Tick; bar: Tick } {
  const segs = meterSegments(map);
  let seg = segs[0]!;
  for (const s of segs) if (s.tick <= tick) seg = s;
  return { tick: seg.tick, bar: ticksPerBar(seg.sig) };
}

/** Start tick of the bar that holds `tick`, following meter changes. */
export function barStartAt(map: TempoMap, tick: Tick): Tick {
  const { tick: from, bar } = segmentAt(map, tick);
  return from + Math.floor((Math.max(0, tick) - from) / bar) * bar;
}

/** First bar line at or after `tick`. */
export function barEndAtOrAfter(map: TempoMap, tick: Tick): Tick {
  const start = barStartAt(map, tick);
  return start === tick ? tick : start + segmentAt(map, tick).bar;
}

/** Copies notes of a clip. Offsets keep each note's place inside its bar, so a paste on any bar line lands on the same beats. */
export function snapshotNotes(map: TempoMap, region: Region, notes: readonly Note[]): NoteShape[] {
  if (notes.length === 0) return [];
  const first = Math.min(...notes.map((n) => region.start + n.start));
  const origin = barStartAt(map, first);
  return notes
    .map((n) => ({ pitch: n.pitch, velocity: n.velocity, offset: region.start + n.start - origin, duration: n.duration }))
    .sort((a, b) => a.offset - b.offset || a.pitch - b.pitch);
}

/** Copies clips; null when none holds notes (audio clips cannot be copied yet). */
export function snapshotClips(items: readonly { track: Id; region: Region }[]): ClipShape[] {
  const midi = items.filter((i) => isMidi(i.region));
  if (midi.length === 0) return [];
  const origin = Math.min(...midi.map((i) => i.region.start));
  return midi
    .map((i) => ({
      track: i.track,
      offset: i.region.start - origin,
      length: i.region.length,
      notes: notesOf(i.region)
        .sort((a, b) => a.start - b.start || a.pitch - b.pitch)
        .map((n) => ({ pitch: n.pitch, velocity: n.velocity, start: n.start, duration: n.duration })),
    }))
    .sort((a, b) => a.offset - b.offset);
}

export interface NotePlan {
  commands: Command[];
  ids: Id[];
  /** Notes that would fall past the end of the clip are kept by growing it; this is the new length, or null. */
  grownTo: Tick | null;
}

interface Placed {
  pitch: number;
  velocity: number;
  start: Tick;
  duration: Tick;
}

/** Adds notes (region relative starts) to a clip, growing the clip to a whole bar first when needed. */
export function addNotesPlan(map: TempoMap, region: Region, placed: readonly Placed[], newId: () => Id, keep: (pitch: number) => number): NotePlan {
  const commands: Command[] = [];
  const end = placed.reduce((m, p) => Math.max(m, p.start + p.duration), 0);
  let grownTo: Tick | null = null;
  if (end > region.length) {
    grownTo = barEndAtOrAfter(map, region.start + end) - region.start;
    commands.push({ command: 'resize_region', region: region.id, length: grownTo });
  }
  const notes = placed.map((p) => ({
    id: newId(),
    pitch: Math.min(127, Math.max(0, keep(p.pitch))),
    velocity: p.velocity,
    start: p.start,
    duration: p.duration,
  }));
  if (notes.length > 0) commands.push({ command: 'add_notes', region: region.id, notes });
  return { commands, ids: notes.map((n) => n.id), grownTo };
}

/** Paste copied notes with the first bar of the copy on the bar line `at` (absolute tick). */
export function pasteNotesPlan(map: TempoMap, region: Region, shapes: readonly NoteShape[], at: Tick, newId: () => Id, keep: (pitch: number) => number): NotePlan {
  const rel = Math.max(0, at - region.start);
  return addNotesPlan(map, region, shapes.map((s) => ({ pitch: s.pitch, velocity: s.velocity, start: rel + s.offset, duration: s.duration })), newId, keep);
}

/** Copies of the notes right after themselves, `times` in a row, each one span later. */
export function repeatNotesPlan(map: TempoMap, region: Region, notes: readonly Note[], times: number, grid: Tick, newId: () => Id, keep: (pitch: number) => number): NotePlan {
  const from = Math.min(...notes.map((n) => n.start));
  const to = Math.max(...notes.map((n) => n.start + n.duration));
  const span = Math.max(grid, Math.ceil((to - from) / grid) * grid);
  const placed: Placed[] = [];
  for (let k = 1; k <= times; k++) {
    for (const n of notes) placed.push({ pitch: n.pitch, velocity: n.velocity, start: n.start + k * span, duration: n.duration });
  }
  return addNotesPlan(map, region, placed, newId, keep);
}

/** Pasted clips as add_region plus add_notes, the first clip starting at `at`, each on its source track when it still exists. */
export function pasteClipsPlan(shapes: readonly ClipShape[], at: Tick, trackExists: (id: Id) => boolean, fallbackTrack: Id, newId: () => Id): { commands: Command[]; ids: Id[]; tracks: Id[] } {
  const commands: Command[] = [];
  const ids: Id[] = [];
  const tracks: Id[] = [];
  for (const s of shapes) {
    const track = trackExists(s.track) ? s.track : fallbackTrack;
    const id = newId();
    commands.push({ command: 'add_region', id, track, start: at + s.offset, length: s.length, kind: 'midi' });
    if (s.notes.length > 0) {
      commands.push({ command: 'add_notes', region: id, notes: s.notes.map((n) => ({ id: newId(), ...n })) });
    }
    ids.push(id);
    tracks.push(track);
  }
  return { commands, ids, tracks };
}

/** How many copies a Repeat may make, from env. */
export function clampRepeat(n: number): number {
  return Math.min(config.maxRepeat, Math.max(1, Math.floor(Number.isFinite(n) ? n : 1)));
}

/** Step labels with plain words. */
export function repeatLabel(times: number): string {
  return `Repeat ${times === 1 ? '1 time' : `${times} times`}`;
}
