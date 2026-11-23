// Drag and drop kit: a typed payload, a target check that answers before the drop, and a tiny
// store that says whether a drag is in flight. Native HTML5 drag carries only a marker type; the
// payload itself lives here because dragover cannot read dataTransfer data.

export type DragPayload =
  | { kind: 'sound'; soundId: string; name: string }
  | { kind: 'sample'; sampleId: string; name: string }
  | { kind: 'file'; fileName: string }
  | { kind: 'clip'; regionIds: readonly string[] }
  | { kind: 'effect'; effectId: string; name: string };

export type DropTarget =
  | { kind: 'track'; trackId: string }
  | { kind: 'clip'; trackId: string; regionId: string }
  | { kind: 'new-track' }
  | { kind: 'none' };

export type DropVerdict = { ok: true; label: string } | { ok: false; reason: string };

export const DRAG_TYPE = 'application/x-composer-drag';

/** Says whether a payload may land on a target, and what the drop will do in plain words. */
export function acceptDrop(payload: DragPayload, target: DropTarget, trackName: (id: string) => string | null): DropVerdict {
  if (target.kind === 'none') return { ok: false, reason: 'Nothing to drop on here' };
  if (payload.kind !== 'sound') return { ok: false, reason: 'Only sounds can be dropped here for now' };
  if (target.kind === 'new-track') return { ok: true, label: `Add a track with ${payload.name}` };
  const name = trackName(target.trackId);
  if (name === null) return { ok: false, reason: 'That track is gone' };
  return { ok: true, label: `Use ${payload.name} on ${name}` };
}

let current: DragPayload | null = null;
const listeners = new Set<() => void>();
const emit = (): void => listeners.forEach((l) => l());

export const subscribeDrag = (l: () => void): (() => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};
export const getDrag = (): DragPayload | null => current;

export interface TransferLike {
  setData(type: string, data: string): void;
  effectAllowed: string;
}

export function beginDrag(payload: DragPayload, transfer: TransferLike | null): void {
  current = payload;
  if (transfer) {
    transfer.setData(DRAG_TYPE, payload.kind);
    transfer.effectAllowed = 'copy';
  }
  emit();
}

export function endDrag(): void {
  if (current === null) return;
  current = null;
  emit();
}

/** Resolves a lane index (and the clip under the pointer, if any) to a drop target. Below the last lane is a new track. */
export function laneTarget(trackIds: readonly string[], lane: number | null, regionId: string | null): DropTarget {
  if (lane === null || lane < 0) return { kind: 'none' };
  const trackId = trackIds[lane];
  if (trackId === undefined) return { kind: 'new-track' };
  return regionId ? { kind: 'clip', trackId, regionId } : { kind: 'track', trackId };
}
