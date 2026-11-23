// "Catch what I just played": a rolling buffer of live notes on a wall clock, and the plan that
// turns it into one new clip. Pure: the clock and ids are passed in, so tests need no browser.
import { newId as defaultNewId } from '../ids';
import type { Command, Id, TempoMap, Tick } from '../model';
import { secondsToTicks, ticksToSeconds } from '../time';
import { recordCommands, type Take } from './recorder';

export interface Played {
  pitch: number;
  velocity: number;
  /** Milliseconds on the buffer clock. */
  start: number;
  /** Null while the key is still down. */
  end: number | null;
  track: Id | null;
}

export interface CatchLimits {
  windowMs: number;
  maxNotes: number;
}

export class CatchBuffer {
  private list: Played[] = [];

  constructor(private readonly limits: CatchLimits) {}

  private trim(now: number): void {
    const floor = now - this.limits.windowMs;
    this.list = this.list.filter((n) => (n.end ?? now) >= floor);
    if (this.list.length > this.limits.maxNotes) this.list = this.list.slice(this.list.length - this.limits.maxNotes);
  }

  noteOn(pitch: number, velocity: number, now: number, track: Id | null): void {
    const open = this.list.find((n) => n.pitch === pitch && n.end === null);
    if (open) open.end = now;
    this.list.push({ pitch, velocity, start: now, end: null, track });
    this.trim(now);
  }

  noteOff(pitch: number, now: number): void {
    for (let i = this.list.length - 1; i >= 0; i--) {
      const n = this.list[i];
      if (n && n.pitch === pitch && n.end === null) {
        n.end = now;
        return;
      }
    }
  }

  count(now: number): number {
    this.trim(now);
    return this.list.length;
  }

  /** Notes in the window, still held ones ended at `now`, ordered by start. */
  notes(now: number): Played[] {
    this.trim(now);
    return this.list.map((n) => ({ ...n, end: n.end ?? now })).sort((a, b) => a.start - b.start || a.pitch - b.pitch);
  }

  clear(): void {
    this.list = [];
  }
}

export interface CatchPlanOptions {
  map: TempoMap;
  track: Id;
  region: Id;
  /** Where the new clip starts. */
  clipStart: Tick;
  bar: Tick;
  grid: Tick | null;
  newId?: () => Id;
}

/** Played notes as a take in absolute ticks, the first note at the clip start. */
export function catchTakes(played: readonly Played[], map: TempoMap, clipStart: Tick): Take[] {
  const first = played.reduce((m, n) => Math.min(m, n.start), Infinity);
  const base = ticksToSeconds(map, clipStart);
  const at = (ms: number): Tick => secondsToTicks(map, base + (ms - first) / 1000);
  return played.map((n) => {
    const start = at(n.start);
    return { pitch: n.pitch, velocity: n.velocity, start, duration: Math.max(1, at(n.end ?? n.start) - start), pass: 0 };
  });
}

/** add_region for a one bar clip, then the notes (the clip grows to whole bars by itself). One group. */
export function catchPlan(played: readonly Played[], o: CatchPlanOptions): Command[] {
  if (played.length === 0) return [];
  const takes = catchTakes(played, o.map, o.clipStart);
  return [
    { command: 'add_region', id: o.region, track: o.track, start: o.clipStart, length: o.bar, kind: 'midi' },
    ...recordCommands(takes, { region: o.region, start: o.clipStart, length: o.bar }, { grid: o.grid, bar: o.bar, newId: o.newId ?? defaultNewId }),
  ];
}

/** The first bar line at or after `start` where a clip of `length` fits between the track's clips. */
export function freeClipStart(clips: readonly { start: Tick; length: Tick }[], start: Tick, length: Tick, bar: Tick): Tick {
  let at = start;
  for (;;) {
    const hit = clips.find((c) => c.start < at + length && at < c.start + c.length);
    if (!hit) return at;
    at = Math.ceil((hit.start + hit.length) / bar) * bar;
  }
}
