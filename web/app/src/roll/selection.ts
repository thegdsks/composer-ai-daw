// Pure helpers for selecting and bulk editing notes: marquee hits, previews that match the core
// result, wording for the status line and the live region. No DOM, so vitest covers them.
import { config } from '../config';
import type { Id, Note, Region, Tick } from '../model';
import { pitchName } from '../time';
import { visibleRange } from './geometry';

export interface MarqueeBox {
  /** Absolute ticks, either order. */
  tickA: Tick;
  tickB: Tick;
  /** Pitch rows touched at the two corners, either order. */
  pitchA: number;
  pitchB: number;
}

/** Ids of the notes a marquee touches. `notes` is sorted by start, `regionStart` makes them absolute. */
export function marqueeHits(
  notes: readonly Note[],
  maxDuration: Tick,
  regionStart: Tick,
  box: MarqueeBox,
): Id[] {
  const t0 = Math.min(box.tickA, box.tickB) - regionStart;
  const t1 = Math.max(box.tickA, box.tickB) - regionStart;
  const p0 = Math.min(box.pitchA, box.pitchB);
  const p1 = Math.max(box.pitchA, box.pitchB);
  const [lo, hi] = visibleRange(notes, maxDuration, t0, t1);
  const out: Id[] = [];
  for (let i = lo; i < hi; i++) {
    const n = notes[i]!;
    if (n.pitch < p0 || n.pitch > p1) continue;
    if (n.start + n.duration <= t0 || n.start > t1) continue;
    out.push(n.id);
  }
  return out;
}

/**
 * Velocity of a note on a linear ramp over the picked notes, same integer arithmetic as the
 * core command so the preview and the result agree.
 */
export function rampValue(from: number, to: number, first: Tick, last: Tick, start: Tick): number {
  const span = last - first;
  if (span === 0) return from;
  return from + Math.trunc(((to - from) * (start - first)) / span);
}

/** New velocity for every note of a ramp, keyed by id. */
export function rampVelocities(notes: readonly Note[], from: number, to: number): Map<Id, number> {
  const out = new Map<Id, number>();
  if (notes.length === 0) return out;
  let first = Infinity;
  let last = -Infinity;
  for (const n of notes) {
    if (n.start < first) first = n.start;
    if (n.start > last) last = n.start;
  }
  for (const n of notes) out.set(n.id, rampValue(from, to, first, last, n.start));
  return out;
}

/** Where quantize puts a note: toward the nearest grid line by `strength` percent, kept inside the clip. */
export function quantizedStart(note: Note, grid: Tick, strength: number, regionLength: Tick): Tick {
  const target = Math.floor((note.start + Math.floor(grid / 2)) / grid) * grid;
  const moved = note.start + Math.trunc(((target - note.start) * strength) / 100);
  return Math.min(moved, regionLength - note.duration);
}

/** Preview of a quantize: the new start of every note that would move. */
export function quantizePreview(
  region: Region,
  notes: readonly Note[],
  grid: Tick,
  strength: number,
): Map<Id, Tick> {
  const out = new Map<Id, Tick>();
  for (const n of notes) {
    const s = quantizedStart(n, grid, strength, region.length);
    if (s !== n.start) out.set(n.id, s);
  }
  return out;
}

/**
 * One delta for a whole group so it moves rigidly: the tick shift is limited by the earliest and
 * latest note, the pitch shift by the lowest and highest.
 */
export function clampGroupDelta(
  region: Region,
  notes: readonly Note[],
  dTick: Tick,
  dPitch: number,
): { dTick: Tick; dPitch: number } {
  if (notes.length === 0) return { dTick: 0, dPitch: 0 };
  let minStart = Infinity;
  let maxEnd = -Infinity;
  let lo = 127;
  let hi = 0;
  for (const n of notes) {
    minStart = Math.min(minStart, n.start);
    maxEnd = Math.max(maxEnd, n.start + n.duration);
    lo = Math.min(lo, n.pitch);
    hi = Math.max(hi, n.pitch);
  }
  return {
    // `|| 0` turns -0 into 0 so callers can compare with ===.
    dTick: Math.min(Math.max(dTick, -minStart), region.length - maxEnd) || 0,
    dPitch: Math.min(Math.max(dPitch, -lo), 127 - hi) || 0,
  };
}

/** Duration change shared by a group: no note may shrink below `min` or run past the clip end. */
export function clampGroupResize(region: Region, notes: readonly Note[], delta: Tick, min: Tick): Tick {
  let lo = -Infinity;
  let hi = Infinity;
  for (const n of notes) {
    lo = Math.max(lo, Math.min(min, n.duration) - n.duration);
    hi = Math.min(hi, region.length - n.start - n.duration);
  }
  return Math.min(Math.max(delta, lo), hi);
}

export type LoudnessName = 'Soft' | 'Medium' | 'Loud';

export function loudnessName(velocity: number): LoudnessName {
  if (velocity <= config.velocitySoftMax) return 'Soft';
  if (velocity >= config.velocityLoudMin) return 'Loud';
  return 'Medium';
}

/** "Loud (112)": the label first, the number for people who want it. */
export function loudnessText(velocity: number): string {
  return `${loudnessName(velocity)} (${velocity})`;
}

/** Midpoint velocity of each name, used by the Soft, Medium and Loud buttons. */
export function loudnessValue(name: LoudnessName): number {
  if (name === 'Soft') return Math.max(1, Math.round(config.velocitySoftMax / 2));
  if (name === 'Loud') return Math.min(127, Math.round((config.velocityLoudMin + 127) / 2));
  return Math.round((config.velocitySoftMax + config.velocityLoudMin) / 2);
}

/** Pitch names of a selection, lowest first, for the status line: "C4, E4, G4 and 2 more". */
export function selectionKeys(notes: readonly Note[], max = config.selectionKeysMax): string {
  const names = [...new Set(notes.map((n) => n.pitch))].sort((a, b) => a - b).map(pitchName);
  if (names.length <= max) return names.join(', ');
  return `${names.slice(0, max).join(', ')} and ${names.length - max} more`;
}

function beatsText(ticks: Tick): string {
  const beats = ticks / config.ticksPerQuarter;
  const shown = Number.isInteger(beats) ? String(beats) : beats.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
  return `${shown} ${beats === 1 ? 'beat' : 'beats'}`;
}

/** What a note cursor move says: pitch, bar, beat and length. */
export function noteSpoken(note: Note, regionStart: Tick, barTicks: Tick): string {
  const at = regionStart + note.start;
  const bar = Math.floor(at / barTicks) + 1;
  const beat = Math.floor((at % barTicks) / config.ticksPerQuarter) + 1;
  return `${pitchName(note.pitch)}, bar ${bar}, beat ${beat}, length ${beatsText(note.duration)}`;
}

/** Selection summary for the status line and the live region. */
export function selectionText(notes: readonly Note[]): string {
  if (notes.length === 0) return '0 selected';
  if (notes.length === 1) return `1 selected (${pitchName(notes[0]!.pitch)})`;
  return `${notes.length} selected (${selectionKeys(notes)})`;
}

const PITCH_CLASS: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };

/** "C4", "f#3", "Bb2" or a plain MIDI number to a pitch, null when it is not one. */
export function parsePitch(text: string): number | null {
  const t = text.trim();
  if (/^\d+$/.test(t)) {
    const n = Number(t);
    return n >= 0 && n <= 127 ? n : null;
  }
  const m = /^([A-Ga-g])([#b]?)(-?\d)$/.exec(t);
  if (!m) return null;
  const base = PITCH_CLASS[m[1]!.toUpperCase()];
  if (base === undefined) return null;
  const pitch = base + (m[2] === '#' ? 1 : m[2] === 'b' ? -1 : 0) + (Number(m[3]) + 1) * 12;
  return pitch >= 0 && pitch <= 127 ? pitch : null;
}

/** Beats typed by a person ("1.5") to ticks, null when it is not a number. */
export function parseBeats(text: string): Tick | null {
  const n = Number(text.trim().replace(',', '.'));
  return text.trim() !== '' && Number.isFinite(n) ? Math.round(n * config.ticksPerQuarter) : null;
}

/** Ticks as beats for a text box, without trailing zeros. */
export function beatsValue(ticks: Tick): string {
  return String(Number((ticks / config.ticksPerQuarter).toFixed(3)));
}

/**
 * Auto grid: the finest listed note size whose cell is still at least `minCellPx` wide at this
 * zoom, so zooming in gives finer snapping and zooming out coarser.
 */
export function autoGridTicks(
  pxPerTick: number,
  sizes: readonly { ticks: Tick }[],
  minCellPx = config.gridMinCellPx,
): Tick {
  const sorted = [...sizes].sort((a, b) => b.ticks - a.ticks);
  let pick = sorted[0]?.ticks ?? config.ticksPerQuarter;
  for (const s of sorted) if (s.ticks * pxPerTick >= minCellPx) pick = s.ticks;
  return pick;
}
