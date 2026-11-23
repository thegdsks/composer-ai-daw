// Piano roll geometry and hit testing. Pure functions over a viewport, no DOM.
import type { Note, Tick } from '../model';

export const MAX_PITCH = 127;

export interface Viewport {
  /** Tick at the left edge of the grid. */
  scrollTick: number;
  /** Pixels from the top of pitch 127 to the top of the grid. */
  scrollY: number;
  pxPerTick: number;
  rowHeight: number;
  /** Grid size in CSS pixels, keyboard and ruler excluded. */
  width: number;
  height: number;
}

export function tickToX(vp: Viewport, tick: number): number {
  return (tick - vp.scrollTick) * vp.pxPerTick;
}

export function xToTick(vp: Viewport, x: number): number {
  return x / vp.pxPerTick + vp.scrollTick;
}

/** Top edge of a pitch row. */
export function pitchToY(vp: Viewport, pitch: number): number {
  return (MAX_PITCH - pitch) * vp.rowHeight - vp.scrollY;
}

export function yToPitch(vp: Viewport, y: number): number {
  const p = MAX_PITCH - Math.floor((y + vp.scrollY) / vp.rowHeight);
  return Math.min(MAX_PITCH, Math.max(0, p));
}

export type HitZone = 'body' | 'edge';

export interface NoteHit {
  note: Note;
  zone: HitZone;
}

/** Topmost note under a grid point. Notes are in region local ticks. */
export function hitNote(
  notes: readonly Note[],
  regionStart: Tick,
  vp: Viewport,
  x: number,
  y: number,
  edgeGrab: number,
): NoteHit | null {
  for (let i = notes.length - 1; i >= 0; i--) {
    const n = notes[i]!;
    const left = tickToX(vp, regionStart + n.start);
    const right = tickToX(vp, regionStart + n.start + n.duration);
    const top = pitchToY(vp, n.pitch);
    if (y < top || y >= top + vp.rowHeight) continue;
    // A short note keeps its body reachable: the edge zone never takes more than half.
    const grab = Math.min(edgeGrab, (right - left) / 2);
    if (x >= left && x <= right) {
      return { note: n, zone: x >= right - grab ? 'edge' : 'body' };
    }
  }
  return null;
}

/** Note whose velocity bar sits under a point of the velocity lane. */
export function hitVelocityBar(
  notes: readonly Note[],
  regionStart: Tick,
  vp: Viewport,
  x: number,
  barWidth: number,
): Note | null {
  let best: Note | null = null;
  let bestDist = Infinity;
  for (const n of notes) {
    const nx = tickToX(vp, regionStart + n.start);
    const d = Math.abs(x - nx - barWidth / 2);
    if (d <= barWidth && d < bestDist) {
      best = n;
      bestDist = d;
    }
  }
  return best;
}

export function velocityFromY(y: number, laneHeight: number): number {
  const v = Math.round((1 - y / laneHeight) * 127);
  return Math.min(127, Math.max(1, v));
}

export interface TickRange {
  from: Tick;
  to: Tick;
}

export function visibleTicks(vp: Viewport): TickRange {
  return { from: vp.scrollTick, to: xToTick(vp, vp.width) };
}

/** Keeps the tick under the pointer fixed while the zoom changes. */
export function zoomAround(vp: Viewport, x: number, pxPerTick: number): Viewport {
  const anchor = xToTick(vp, x);
  return { ...vp, pxPerTick, scrollTick: anchor - x / pxPerTick };
}

/**
 * Index range [lo, hi) of notes (sorted by start, region local ticks) that can touch the
 * region local span [from, to). `maxDuration` bounds how far back a long note can reach.
 */
export function visibleRange(
  notes: readonly Note[],
  maxDuration: number,
  from: Tick,
  to: Tick,
): [number, number] {
  const lowerBound = (target: number): number => {
    let lo = 0;
    let hi = notes.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if ((notes[mid]?.start ?? 0) < target) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  };
  return [lowerBound(from - maxDuration), lowerBound(to)];
}

// ---- Arrangement: lanes, clips, view math and the scrollbar. Pure, no DOM. ----

export interface ClipSpan {
  id: string;
  start: Tick;
  length: Tick;
}

export interface LaneView {
  /** Pixels from the canvas top to the first lane (the ruler). */
  top: number;
  rowHeight: number;
  scrollY: number;
  laneCount: number;
}

export interface LaneHit {
  /** Lane index; equals laneCount for the empty strip below the last lane. */
  lane: number;
  below: boolean;
}

/** Lane under a canvas y, null over the ruler. Below the last lane means "a new track". */
export function laneAt(v: LaneView, y: number): LaneHit | null {
  if (y < v.top) return null;
  const i = Math.floor((y - v.top + v.scrollY) / v.rowHeight);
  if (i < 0) return null;
  if (i >= v.laneCount) return { lane: v.laneCount, below: true };
  return { lane: i, below: false };
}

/** Lane to drop on while dragging: over the ruler clamps to the first lane. */
export function dropLane(v: LaneView, y: number): number {
  return laneAt(v, y)?.lane ?? 0;
}

export type ClipZone = 'body' | 'edge' | 'trim';

export interface ClipHit {
  clip: ClipSpan;
  zone: ClipZone;
}

/** Topmost clip of one lane at a pixel; the right edge strip resizes, the left strip (when asked for) trims. */
export function hitClip(
  clips: readonly ClipSpan[],
  scrollTick: number,
  pxPerTick: number,
  px: number,
  edgeGrab: number,
  leftGrab = 0,
): ClipHit | null {
  for (let i = clips.length - 1; i >= 0; i--) {
    const c = clips[i]!;
    const left = (c.start - scrollTick) * pxPerTick;
    const width = c.length * pxPerTick;
    if (px < left || px >= left + width) continue;
    const grab = Math.min(edgeGrab, width / 3);
    const lgrab = Math.min(leftGrab, width / 3);
    if (leftGrab > 0 && px < left + lgrab) return { clip: c, zone: 'trim' };
    return { clip: c, zone: px >= left + width - grab ? 'edge' : 'body' };
  }
  return null;
}

export interface FitView {
  pxPerTick: number;
  scrollTick: number;
}

/** Zoom and scroll that show [from, to) in a canvas of `width` px with `pad` px each side. */
export function fitView(from: Tick, to: Tick, width: number, pad: number, minPx: number, maxPx: number): FitView {
  const span = Math.max(1, to - from);
  const usable = Math.max(1, width - pad * 2);
  const pxPerTick = Math.min(maxPx, Math.max(minPx, usable / span));
  return { pxPerTick, scrollTick: Math.max(0, from - pad / pxPerTick) };
}

export interface Thumb {
  x: number;
  width: number;
}

/** Scrollbar thumb for a view of `span` ticks inside `total`; null when everything fits. */
export function scrollThumb(scrollTick: number, span: number, total: number, trackPx: number, minThumb: number): Thumb | null {
  if (total <= span || trackPx <= 0) return null;
  const width = Math.min(trackPx, Math.max(minThumb, (span / total) * trackPx));
  const room = trackPx - width;
  const x = (scrollTick / (total - span)) * room;
  return { x: Math.min(room, Math.max(0, x)), width };
}

/** Scroll position for a thumb whose left edge is at x. */
export function scrollFromThumb(x: number, thumbWidth: number, span: number, total: number, trackPx: number): number {
  const room = Math.max(1, trackPx - thumbWidth);
  return Math.min(1, Math.max(0, x / room)) * Math.max(0, total - span);
}

/** One based first and last bar a clip covers, for the live region ("bars 5 to 8"). */
export function clipBars(start: Tick, length: Tick, barTicks: Tick): { first: number; last: number } {
  const first = Math.floor(start / barTicks) + 1;
  const last = Math.max(first, Math.ceil((start + length) / barTicks));
  return { first, last };
}

export function barsText(first: number, last: number): string {
  return first === last ? `bar ${first}` : `bars ${first} to ${last}`;
}

/** True when the clip shares time with another clip of the same lane (allowed, but worth saying). */
export function overlapsOther(clips: readonly ClipSpan[], movedId: string): boolean {
  const m = clips.find((c) => c.id === movedId);
  if (!m) return false;
  return clips.some((c) => c.id !== movedId && c.start < m.start + m.length && m.start < c.start + c.length);
}

/** Neighbour of a clip in start order: -1 previous, 1 next; null at the ends. */
export function neighbourClip(clips: readonly ClipSpan[], fromId: string, dir: -1 | 1): ClipSpan | null {
  const sorted = [...clips].sort((a, b) => a.start - b.start || (a.id < b.id ? -1 : 1));
  const i = sorted.findIndex((c) => c.id === fromId);
  if (i < 0) return null;
  return sorted[i + dir] ?? null;
}

/** Clip of a lane closest to a tick: the one covering it, else the nearest by edge. */
export function clipNearTick(clips: readonly ClipSpan[], tick: Tick): ClipSpan | null {
  let best: ClipSpan | null = null;
  let bestDist = Infinity;
  for (const c of clips) {
    const inside = tick >= c.start && tick < c.start + c.length;
    const d = inside ? 0 : Math.min(Math.abs(tick - c.start), Math.abs(tick - (c.start + c.length)));
    if (d < bestDist) {
      best = c;
      bestDist = d;
    }
  }
  return best;
}
