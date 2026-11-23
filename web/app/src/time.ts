// Pure time and grid math: ticks, bars from the meter map, seconds from the tempo map.
import { config } from './config';
import type { TempoMap, Tick, TimeSignature } from './model';

export function snapRound(tick: Tick, grid: Tick): Tick {
  return Math.round(tick / grid) * grid;
}

export function snapFloor(tick: Tick, grid: Tick): Tick {
  return Math.floor(tick / grid) * grid;
}

export function ticksPerBar(sig: TimeSignature, tpq = config.ticksPerQuarter): Tick {
  return (sig.numerator * 4 * tpq) / sig.denominator;
}

export function ticksPerBeat(sig: TimeSignature, tpq = config.ticksPerQuarter): Tick {
  return (4 * tpq) / sig.denominator;
}

interface MeterSegment {
  tick: Tick;
  sig: TimeSignature;
  /** Zero based number of the first bar that starts in this segment. */
  firstBar: number;
}

/** Meter changes in tick order, each with the bar number it starts at. */
export function meterSegments(map: TempoMap, tpq = config.ticksPerQuarter): MeterSegment[] {
  const entries = Object.entries(map.meters)
    .map(([t, sig]) => ({ tick: Number(t), sig }))
    .sort((a, b) => a.tick - b.tick);
  if (entries.length === 0) entries.push({ tick: 0, sig: { numerator: 4, denominator: 4 } });
  const out: MeterSegment[] = [];
  let bar = 0;
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i]!;
    out.push({ tick: e.tick, sig: e.sig, firstBar: bar });
    const next = entries[i + 1];
    if (next) bar += Math.ceil((next.tick - e.tick) / ticksPerBar(e.sig, tpq));
  }
  return out;
}

export interface Line {
  tick: Tick;
  /** Zero based bar index this line belongs to. */
  bar: number;
  /** True for a bar line, false for a beat line inside a bar. */
  isBar: boolean;
}

/** Bar and beat lines that fall in [from, to). Beat lines are skipped when `beats` is false. */
export function barLines(
  map: TempoMap,
  from: Tick,
  to: Tick,
  beats: boolean,
  tpq = config.ticksPerQuarter,
): Line[] {
  const segs = meterSegments(map, tpq);
  const out: Line[] = [];
  for (let i = 0; i < segs.length; i++) {
    const seg = segs[i]!;
    const end = segs[i + 1]?.tick ?? Infinity;
    if (end <= from || seg.tick >= to) continue;
    const barLen = ticksPerBar(seg.sig, tpq);
    const beatLen = ticksPerBeat(seg.sig, tpq);
    const firstIdx = Math.max(0, Math.floor((from - seg.tick) / barLen));
    for (let k = firstIdx; ; k++) {
      const barTick = seg.tick + k * barLen;
      if (barTick >= to || barTick >= end) break;
      if (barTick >= from) out.push({ tick: barTick, bar: seg.firstBar + k, isBar: true });
      if (beats) {
        for (let b = beatLen; b < barLen; b += beatLen) {
          const t = barTick + b;
          if (t >= end) break;
          if (t >= from && t < to) out.push({ tick: t, bar: seg.firstBar + k, isBar: false });
        }
      }
    }
  }
  return out;
}

interface TempoSegment {
  tick: Tick;
  /** Seconds at the start of the segment. */
  seconds: number;
  secondsPerTick: number;
}

function tempoSegments(map: TempoMap, tpq: number): TempoSegment[] {
  const entries = Object.entries(map.tempos)
    .map(([t, mbpm]) => ({ tick: Number(t), mbpm }))
    .sort((a, b) => a.tick - b.tick);
  if (entries.length === 0) entries.push({ tick: 0, mbpm: 120000 });
  const out: TempoSegment[] = [];
  let seconds = 0;
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i]!;
    const spt = 60000 / (e.mbpm * tpq);
    out.push({ tick: e.tick, seconds, secondsPerTick: spt });
    const next = entries[i + 1];
    if (next) seconds += (next.tick - e.tick) * spt;
  }
  return out;
}

export function ticksToSeconds(map: TempoMap, tick: Tick, tpq = config.ticksPerQuarter): number {
  const segs = tempoSegments(map, tpq);
  let seg = segs[0]!;
  for (const s of segs) if (s.tick <= tick) seg = s;
  return seg.seconds + (tick - seg.tick) * seg.secondsPerTick;
}

export function secondsToTicks(map: TempoMap, seconds: number, tpq = config.ticksPerQuarter): Tick {
  const segs = tempoSegments(map, tpq);
  let seg = segs[0]!;
  for (const s of segs) if (s.seconds <= seconds) seg = s;
  return seg.tick + (seconds - seg.seconds) / seg.secondsPerTick;
}

export function bpmAt(map: TempoMap, tick: Tick): number {
  let best = 0;
  let mbpm = 120000;
  for (const [t, v] of Object.entries(map.tempos)) {
    const n = Number(t);
    if (n <= tick && n >= best) {
      best = n;
      mbpm = v;
    }
  }
  return mbpm / 1000;
}

export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

/** Label of a MIDI pitch such as C4 (middle C is 60). */
export function pitchName(pitch: number): string {
  const names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
  return `${names[pitch % 12]}${Math.floor(pitch / 12) - 1}`;
}

export function isBlackKey(pitch: number): boolean {
  return [1, 3, 6, 8, 10].includes(pitch % 12);
}

/** One metronome click: where it lands in sample frames and whether it opens a bar. */
export interface Click {
  frame: number;
  accent: boolean;
}

/** Beat clicks for [fromTick, toTick) with frames counted from tick 0 of the song. */
export function beatClicks(map: TempoMap, fromTick: Tick, toTick: Tick, sampleRate: number, tpq = config.ticksPerQuarter): Click[] {
  return barLines(map, fromTick, toTick, true, tpq).map((l) => ({
    frame: Math.round(ticksToSeconds(map, l.tick, tpq) * sampleRate),
    accent: l.isBar,
  }));
}

/** Count in clicks as frame offsets from the moment Play is pressed; the bar length follows the meter at `tick`. */
export function countInClicks(map: TempoMap, tick: Tick, bars: number, sampleRate: number, tpq = config.ticksPerQuarter): { clicks: Click[]; totalFrames: number } {
  const segs = meterSegments(map, tpq);
  let sig = segs[0]?.sig ?? { numerator: 4, denominator: 4 };
  for (const s of segs) if (s.tick <= tick) sig = s.sig;
  const beats = Math.max(0, Math.round(bars)) * sig.numerator;
  const beatSec = 60 / bpmAt(map, tick) / (sig.denominator / 4);
  const clicks: Click[] = [];
  for (let i = 0; i < beats; i++) clicks.push({ frame: Math.round(i * beatSec * sampleRate), accent: i % sig.numerator === 0 });
  return { clicks, totalFrames: Math.round(beats * beatSec * sampleRate) };
}

export interface LoopTicks {
  on: boolean;
  start: Tick;
  end: Tick;
}

/** A loop only counts when it is on and spans something. */
export function loopActive(loop: LoopTicks): boolean {
  return loop.on && loop.end > loop.start;
}

/** Where the playhead lands after moving to `tick`: past the loop end it folds back into the loop. */
export function wrapTick(tick: Tick, loop: LoopTicks): Tick {
  if (!loopActive(loop) || tick < loop.end) return tick;
  return loop.start + ((tick - loop.start) % (loop.end - loop.start));
}

/** Frames of the next audio segment: it stops at the loop end so the wrap lands on the exact sample. */
export function segmentFrames(position: number, wanted: number, loopEndFrame: number | null): number {
  if (loopEndFrame === null || position >= loopEndFrame) return wanted;
  return Math.min(wanted, loopEndFrame - position);
}

/** Ruler click or drag point to a tick: snapped to the step, never before the song start. */
export function rulerTick(px: number, scrollTick: number, pxPerTick: number, step: Tick): Tick {
  return Math.max(0, snapRound(px / pxPerTick + scrollTick, step));
}

/** Drag on the ruler from one point to another: ordered, snapped, at least one step long. */
export function braceFromDrag(a: Tick, b: Tick, step: Tick): { start: Tick; end: Tick } {
  const lo = Math.max(0, snapRound(Math.min(a, b), step));
  let hi = snapRound(Math.max(a, b), step);
  if (hi - lo < step) hi = lo + step;
  return { start: lo, end: hi };
}

/** Zero based bar number that holds `tick`, following meter changes. */
export function barAt(map: TempoMap, tick: Tick, tpq = config.ticksPerQuarter): number {
  const segs = meterSegments(map, tpq);
  let seg = segs[0]!;
  for (const s of segs) if (s.tick <= tick) seg = s;
  return seg.firstBar + Math.floor((tick - seg.tick) / ticksPerBar(seg.sig, tpq));
}

/** "bars 1 to 4" for a range whose end is exclusive, "bar 3" when it covers one bar. */
export function barRangeText(map: TempoMap, start: Tick, end: Tick): string {
  const first = barAt(map, start) + 1;
  const last = Math.max(first, barAt(map, Math.max(start, end - 1)) + 1);
  return first === last ? `bar ${first}` : `bars ${first} to ${last}`;
}

/** Index of the first value that is at least `target` in a sorted array. */
export function lowerBound(sorted: ArrayLike<number>, target: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if ((sorted[mid] as number) < target) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}
