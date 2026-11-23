// What dropping a sound does. Every branch is one undo step built from existing commands.
import { newId } from '../ids';
import { INSTRUMENT_SINE } from '../model';
import { meterSegments } from '../time';
import type { DropTarget } from '../ui/dnd';
import type { Actions } from './actions';
import { defaultRegionLength } from './commands';
import type { Store } from './store';

/** A new track named after the sound, with a first clip, in one group. Returns false when it failed. */
export async function addTrackWithSound(store: Store, soundId: string, name: string): Promise<boolean> {
  const project = store.getSnapshot().project;
  const id = newId();
  const region = newId();
  const sig = meterSegments(project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 };
  const r = await store.group(`Add track with ${name}`, [
    { command: 'add_track', id, name, kind: 'instrument', index: null },
    { command: 'set_instrument', track: id, instrument: INSTRUMENT_SINE },
    { command: 'set_track_sound', track: id, sound: soundId },
    { command: 'add_region', id: region, track: id, start: 0, length: defaultRegionLength(sig), kind: 'midi' },
  ]);
  if (r.ok) store.selectRegion(id, region);
  return r.ok;
}

/** Applies a sound drop (or its keyboard twin) to a resolved target. */
export async function dropSound(
  deps: { actions: Actions; store: Store },
  sound: { soundId: string; name: string },
  target: DropTarget,
): Promise<boolean> {
  switch (target.kind) {
    case 'track':
    case 'clip': {
      const ok = await deps.actions.setSound(target.trackId, sound.soundId);
      if (ok) deps.store.selectTrack(target.trackId);
      return ok;
    }
    case 'new-track':
      return addTrackWithSound(deps.store, sound.soundId, sound.name);
    default:
      return false;
  }
}

/** Moves a track one place up (-1) or down (+1); one undo step. */
export async function moveTrackBy(store: Store, trackId: string, delta: -1 | 1): Promise<boolean> {
  const tracks = store.getSnapshot().project.tracks;
  const at = tracks.findIndex((t) => t.id === trackId);
  const to = at + delta;
  if (at < 0 || to < 0 || to >= tracks.length) return false;
  const r = await store.apply({ command: 'move_track', track: trackId, index: to });
  return r.ok;
}
