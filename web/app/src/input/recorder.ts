// Pure recording math: where playback is on the audio clock, notes captured per loop pass,
// and the commands that put a take into a clip. No audio, DOM or store here, so it is
// testable with a fake clock.
import { newId as defaultNewId } from '../ids';
import type { Command, Id, TempoMap, Tick } from '../model';
import { loopActive, secondsToTicks, snapRound, ticksToSeconds, type LoopTicks } from '../time';
import { clampPitch, clampVelocity } from '../state/commands';

/** Playback was at `tick` when the audio clock read `clock` seconds, in loop pass `pass`. */
export interface ClockAnchor {
  clock: number;
  tick: Tick;
  pass: number;
}

export interface CapturePos {
  tick: Tick;
  pass: number;
}

/**
 * Where playback is at an audio clock time, extrapolated from the last anchor. Elapsed time runs
 * through the tempo map, and a loop folds the position back and counts the passes.
 */
export function locate(map: TempoMap, anchor: ClockAnchor, clock: number, loop: LoopTicks): CapturePos {
  const from = ticksToSeconds(map, anchor.tick);
  const elapsed = clock - anchor.clock;
  if (loopActive(loop) && anchor.tick < loop.end && elapsed > 0) {
    const endSec = ticksToSeconds(map, loop.end);
    const startSec = ticksToSeconds(map, loop.start);
    const room = endSec - from;
    if (elapsed >= room) {
      const extra = elapsed - room;
      const len = endSec - startSec;
      const passes = 1 + Math.floor(extra / len);
      return { tick: Math.max(0, secondsToTicks(map, startSec + (extra % len))), pass: anchor.pass + passes };
    }
  }
  return { tick: Math.max(0, secondsToTicks(map, from + elapsed)), pass: anchor.pass };
}

/** A captured note in absolute song ticks. */
export interface Take {
  pitch: number;
  velocity: number;
  start: Tick;
  duration: Tick;
  pass: number;
}

interface Held {
  velocity: number;
  start: Tick;
  pass: number;
}

/** Collects notes as keys go down and up; a note held across a loop end is split at the wrap. */
export class TakeBuffer {
  private readonly held = new Map<number, Held>();
  private done: Take[] = [];

  constructor(private readonly loop: { start: Tick; end: Tick } | null) {}

  get heldCount(): number {
    return this.held.size;
  }

  get doneCount(): number {
    return this.done.length;
  }

  private close(pitch: number, h: Held, end: Tick): void {
    this.done.push({ pitch, velocity: h.velocity, start: h.start, duration: Math.max(1, Math.round(end - h.start)), pass: h.pass });
  }

  /** Splits held notes that were started in an earlier pass: they end at the loop end and restart at its start. */
  advance(pos: CapturePos): void {
    if (!this.loop) return;
    for (const [pitch, h] of this.held) {
      if (h.pass >= pos.pass) continue;
      this.close(pitch, h, this.loop.end);
      this.held.set(pitch, { velocity: h.velocity, start: this.loop.start, pass: pos.pass });
    }
  }

  noteOn(pitch: number, velocity: number, pos: CapturePos): void {
    this.advance(pos);
    const prev = this.held.get(pitch);
    if (prev) this.close(pitch, prev, pos.tick);
    this.held.set(pitch, { velocity, start: pos.tick, pass: pos.pass });
  }

  noteOff(pitch: number, pos: CapturePos): void {
    this.advance(pos);
    const h = this.held.get(pitch);
    if (!h) return;
    this.held.delete(pitch);
    this.close(pitch, h, pos.tick);
  }

  /** Finished notes of passes before `pass`; they leave the buffer. */
  takeBefore(pass: number): Take[] {
    const out = this.done.filter((t) => t.pass < pass);
    this.done = this.done.filter((t) => t.pass >= pass);
    return out;
  }

  /** Ends every held note at the position and returns everything left. */
  finish(pos: CapturePos): Take[] {
    this.advance(pos);
    for (const [pitch, h] of this.held) this.close(pitch, h, pos.tick);
    this.held.clear();
    const out = this.done;
    this.done = [];
    return out;
  }
}

export interface RecordTarget {
  region: Id;
  start: Tick;
  length: Tick;
}

export interface RecordOptions {
  /** Grid step for tidy timing, or null to keep the played timing. */
  grid: Tick | null;
  /** Ticks in a bar; the clip grows to a whole number of bars when a take runs past its end. */
  bar: Tick;
  newId?: () => Id;
}

export function recordLabel(count: number): string {
  return `Record ${count} ${count === 1 ? 'note' : 'notes'}`;
}

/** The commands for one take: grow the clip if needed, then one add_notes with every note. */
export function recordCommands(takes: readonly Take[], target: RecordTarget, o: RecordOptions): Command[] {
  const newId = o.newId ?? defaultNewId;
  const notes: { pitch: number; velocity: number; start: Tick; duration: Tick }[] = [];
  let end = 0;
  for (const t of takes) {
    let start = Math.max(0, t.start - target.start);
    let duration = t.duration;
    if (o.grid) {
      start = snapRound(start, o.grid);
      duration = Math.max(o.grid, snapRound(duration, o.grid));
    }
    start = Math.round(start);
    duration = Math.max(1, Math.round(duration));
    notes.push({ pitch: t.pitch, velocity: t.velocity, start, duration });
    end = Math.max(end, start + duration);
  }
  const cmds: Command[] = [];
  if (notes.length > 0 && end > target.length) {
    cmds.push({ command: 'resize_region', region: target.region, length: Math.ceil(end / o.bar) * o.bar });
  }
  if (notes.length > 0) {
    cmds.push({
      command: 'add_notes',
      region: target.region,
      notes: notes.map((n) => ({ id: newId(), pitch: clampPitch(n.pitch), velocity: clampVelocity(n.velocity), start: n.start, duration: n.duration })),
    });
  }
  return cmds;
}
