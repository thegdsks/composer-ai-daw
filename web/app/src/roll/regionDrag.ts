// Pure math for dragging and resizing regions in the arrangement, kept apart from the canvas
// so it can be tested. Ticks in, ticks out; the controller turns pixels into ticks.
import { config } from '../config';
import { notesOf, type Region, type Tick, type TimeSignature } from '../model';
import { snapRound, ticksPerBar, ticksPerBeat } from '../time';

export type RegionZone = 'move' | 'resize';

/** Right edge grabs resize, everything else moves. Narrow regions keep half their width for moving. */
export function regionZone(px: number, left: number, width: number, grab = config.regionEdgeGrab): RegionZone {
  const zone = Math.min(grab, width / 2);
  return px >= left + width - zone ? 'resize' : 'move';
}

/**
 * Snap step for clip edits: a bar while a bar is wide enough on screen, else a beat. Free
 * (Shift held) is one tick, so the clip lands exactly where the pointer is.
 */
export function snapStep(sig: TimeSignature, pxPerTick: number, free: boolean, minBarPx = config.snapBarMinPx): Tick {
  if (free) return 1;
  const bar = ticksPerBar(sig);
  return bar * pxPerTick >= minBarPx ? bar : ticksPerBeat(sig);
}

export function movedStart(region: Region, dTick: Tick, grid: Tick): Tick {
  return Math.max(0, snapRound(region.start + dTick, grid));
}

/** A region cannot end before its last note (the core refuses it), nor be shorter than one tick. */
export function minRegionLength(region: Region): Tick {
  let end = 1;
  for (const n of notesOf(region)) end = Math.max(end, n.start + n.duration);
  return end;
}

export function resizedLength(region: Region, dTick: Tick, grid: Tick): Tick {
  const end = snapRound(region.start + region.length + dTick, grid);
  return Math.max(minRegionLength(region), end - region.start);
}
