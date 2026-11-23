// Which timeline lanes are shown. A browser preference, hidden by default so a new project
// stays calm. Storage may be blocked, so every access is guarded.
import { config } from '../config';

export type LaneId = 'markers' | 'tempo' | 'key';
export const LANE_IDS: readonly LaneId[] = ['markers', 'tempo', 'key'];
export const LANE_LABELS: Record<LaneId, string> = { markers: 'Markers', tempo: 'Tempo', key: 'Key' };

function read(): ReadonlySet<LaneId> {
  try {
    const raw = localStorage.getItem(config.laneStorageKey);
    const list = raw ? (JSON.parse(raw) as unknown) : [];
    if (Array.isArray(list)) return new Set(LANE_IDS.filter((id) => list.includes(id)));
  } catch {
    // Blocked or damaged storage: no lanes.
  }
  return new Set();
}

let current: ReadonlySet<LaneId> = read();
const listeners = new Set<() => void>();

export function getLanes(): ReadonlySet<LaneId> {
  return current;
}

export function subscribeLanes(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function toggleLane(id: LaneId): void {
  const next = new Set(current);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  current = next;
  try {
    localStorage.setItem(config.laneStorageKey, JSON.stringify([...next]));
  } catch {
    // Not remembered, still applied for this session.
  }
  for (const fn of listeners) fn();
}

/** Lanes in fixed order, for drawing and labels. */
export function visibleLanes(set: ReadonlySet<LaneId>): LaneId[] {
  return LANE_IDS.filter((id) => set.has(id));
}
