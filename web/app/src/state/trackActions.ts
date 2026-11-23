// Store bound track header edits. Each is one undo step built from existing commands.
import { newId } from '../ids';
import type { Id } from '../model';
import type { Store } from './store';
import { DUPLICATE_LABEL, duplicateTrackPlan, reorderTarget } from './trackPlans';

/** Copies a track with its sound, mix and clips right after it. Returns the new track id or null. */
export async function duplicateTrack(store: Store, trackId: Id): Promise<Id | null> {
  const tracks = store.getSnapshot().project.tracks;
  const source = tracks.find((t) => t.id === trackId);
  if (!source) return null;
  const plan = duplicateTrackPlan(tracks, source, newId);
  const r = await store.group(DUPLICATE_LABEL, plan.commands);
  if (!r.ok) return null;
  if (plan.firstRegion) store.selectRegion(plan.track, plan.firstRegion);
  else store.selectTrack(plan.track);
  return plan.track;
}

/** Moves a track to a row; one move_track, one undo step. Returns false when nothing moved. */
export async function moveTrackTo(store: Store, trackId: Id, slot: number): Promise<boolean> {
  const tracks = store.getSnapshot().project.tracks;
  const to = reorderTarget(
    tracks.findIndex((t) => t.id === trackId),
    slot,
    tracks.length,
  );
  if (to === null) return false;
  const r = await store.apply({ command: 'move_track', track: trackId, index: to });
  return r.ok;
}
