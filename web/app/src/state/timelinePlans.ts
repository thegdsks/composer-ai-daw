// Plans for the marker, tempo and key lanes: which command a gesture sends. Pure, so the UI
// only turns a click into a tick and sends the result.
import { config } from '../config';
import type { Command, Project, ScaleName, TempoMap, Tick } from '../model';
import { lowerBound } from '../time';

export interface LanePoint {
  tick: Tick;
  label: string;
}

export function markerPoints(project: Pick<Project, 'markers'>): LanePoint[] {
  return Object.entries(project.markers ?? {})
    .map(([t, name]) => ({ tick: Number(t), label: name }))
    .sort((a, b) => a.tick - b.tick);
}

export function tempoPoints(map: TempoMap): LanePoint[] {
  return Object.entries(map.tempos)
    .map(([t, mbpm]) => ({ tick: Number(t), label: String(Math.round(mbpm / 10) / 100) }))
    .sort((a, b) => a.tick - b.tick);
}

export function keyPoints(map: TempoMap, name: (root: number, scale: ScaleName) => string): LanePoint[] {
  return Object.entries(map.keys ?? {})
    .map(([t, k]) => ({ tick: Number(t), label: name(k.root, k.scale) }))
    .sort((a, b) => a.tick - b.tick);
}

/** "Marker 3": the first number not used as a marker name yet. */
export function nextMarkerName(project: Pick<Project, 'markers'>): string {
  const used = new Set(Object.values(project.markers ?? {}));
  for (let n = Object.keys(project.markers ?? {}).length + 1; ; n++) {
    const name = `Marker ${n}`;
    if (!used.has(name)) return name;
  }
}

/** Adds a marker; null when one is already there (a double click on it renames instead). */
export function addMarker(project: Pick<Project, 'markers'>, tick: Tick, name?: string): Command | null {
  if (project.markers?.[String(tick)] !== undefined) return null;
  return { command: 'set_marker', tick, name: (name ?? '').trim() || nextMarkerName(project) };
}

/** Renames the marker at a tick; null when the name is empty or unchanged. */
export function renameMarker(project: Pick<Project, 'markers'>, tick: Tick, name: string): Command | null {
  const next = name.trim();
  if (!next || project.markers?.[String(tick)] === next) return null;
  return { command: 'set_marker', tick, name: next };
}

export function removeMarker(tick: Tick): Command {
  return { command: 'remove_marker', tick };
}

/** A tempo point at a tick; the BPM is clamped to the configured range. */
export function tempoAt(tick: Tick, bpm: number): Command | null {
  if (!Number.isFinite(bpm)) return null;
  const clamped = Math.min(config.tempoMaxBpm, Math.max(config.tempoMinBpm, bpm));
  return { command: 'set_tempo', tick, mbpm: Math.round(clamped * 1000) };
}

/** The first tempo point (tick 0) always stays, the core needs a tempo at the start. */
export function removeTempoAt(tick: Tick): Command | null {
  return tick > 0 ? { command: 'remove_tempo', tick } : null;
}

export function keyChange(tick: Tick, root: number, scale: ScaleName): Command {
  return { command: 'set_key', tick, root, scale };
}

/** Index of the point nearest to a pixel within `grab` px, or -1. Points are sorted by tick. */
export function hitPoint(points: readonly LanePoint[], px: number, scrollTick: number, pxPerTick: number, grab = config.laneGrabPx): number {
  if (points.length === 0) return -1;
  const ticks = points.map((p) => p.tick);
  const i = lowerBound(ticks, px / pxPerTick + scrollTick);
  let best = -1;
  let bestDist = Infinity;
  for (const j of [i - 1, i]) {
    const p = points[j];
    if (!p) continue;
    const d = Math.abs((p.tick - scrollTick) * pxPerTick - px);
    if (d < bestDist) {
      best = j;
      bestDist = d;
    }
  }
  return bestDist <= grab ? best : -1;
}

/** Loop lengths shorter than the clip, from the configured beat counts. */
export function loopChoices(beatTicks: Tick, clipLength: Tick, beats: readonly number[] = config.loopChoicesBeats): { beats: number; ticks: Tick }[] {
  return beats.map((b) => ({ beats: b, ticks: Math.round(b * beatTicks) })).filter((c) => c.ticks < clipLength);
}

export function setLoop(region: string, loopLength: Tick): Command {
  return { command: 'set_region_loop', region, loop_length: Math.max(0, Math.round(loopLength)) };
}

/** Plain text for a clip loop: "Off", "1 bar", "6 beats". */
export function loopText(loop: Tick, beatTicks: Tick, barTicks: Tick): string {
  if (loop <= 0) return 'Off';
  if (loop % barTicks === 0) {
    const bars = loop / barTicks;
    return bars === 1 ? '1 bar' : `${bars} bars`;
  }
  const beats = Math.round((loop / beatTicks) * 100) / 100;
  return beats === 1 ? '1 beat' : `${beats} beats`;
}
