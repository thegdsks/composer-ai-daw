import { useCallback, useState, useSyncExternalStore, type DragEvent, type ReactNode, type RefObject } from 'react';
import type { ArrangementController, ClipMenuTarget } from '../roll/ArrangementController';
import type { Id, Track } from '../model';
import type { Store } from '../state/store';
import { dropSound, moveTrackBy } from '../state/soundDrop';
import { duplicateTrack } from '../state/trackActions';
import { ContextMenu, useMenuTrigger } from './ContextMenu';
import { Dialog } from './Dialog';
import { acceptDrop, getDrag, subscribeDrag, type DropTarget } from './dnd';
import { clipMenuItems, trackMenuItems } from './menus';
import { field } from './classes';
import { useSession, useSnapshot } from './hooks';
import { useClipLoop } from './useClipLoop';

const TRACK_ATTR = 'data-track-id';

function trackAt(store: Store, origin: HTMLElement): Track | null {
  const id = origin.closest<HTMLElement>(`[${TRACK_ATTR}]`)?.getAttribute(TRACK_ATTR);
  return store.getSnapshot().project.tracks.find((t) => t.id === id) ?? null;
}

/**
 * Context menus (track header and clip) and sound drops for the arrangement. The panel spreads
 * `listProps` on the track list and `laneProps` on the lane canvas wrapper and renders `menu`.
 */
export function useArrangementMenus(ctrl: RefObject<ArrangementController | null>, openClip: (id: Id) => void) {
  const { actions, store } = useSession();
  const snap = useSnapshot();
  const [renaming, setRenaming] = useState<Track | null>(null);
  const [draft, setDraft] = useState('');
  const [hint, setHint] = useState<string | null>(null);
  const clipLoop = useClipLoop();
  const dragging = useSyncExternalStore(subscribeDrag, getDrag) !== null;

  const trackOf = (origin: HTMLElement): Track | null => trackAt(store, origin);

  const build = useCallback(
    (origin: HTMLElement) => {
      const track = trackAt(store, origin);
      if (!track) return null;
      const tracks = store.getSnapshot().project.tracks;
      store.selectTrack(track.id);
      return {
        label: `Track ${track.name}`,
        items: trackMenuItems(track, tracks.indexOf(track), tracks.length, {
          rename: (t) => {
            setDraft(t.name);
            setRenaming(t);
          },
          move: (id, delta) => void moveTrackBy(store, id, delta),
          duplicate: (t) => void duplicateTrack(store, t.id),
          remove: (t) => {
            void actions.deleteTrack(t.id);
          },
        }),
      };
    },
    [actions, store],
  );
  const trigger = useMenuTrigger(build);

  /** The header menu button: the same menu as a right click, anchored under the button. */
  const openTrackMenu = useCallback(
    (button: HTMLElement) => {
      const made = build(button);
      if (!made) return;
      const box = button.getBoundingClientRect();
      trigger.openAt({ ...made, x: box.left, y: box.bottom });
    },
    [build, trigger],
  );

  const onClipMenu = useCallback(
    (target: ClipMenuTarget, at: { x: number; y: number }) => {
      trigger.openAt({
        label: 'Clip',
        x: at.x,
        y: at.y,
        items: clipMenuItems(target.ids.length, {
          open: () => openClip(target.regionId),
          split: () => void ctrl.current?.splitAtPlayhead(),
          duplicate: () => ctrl.current?.duplicateSelection(),
          remove: () => void actions.deleteClips(target.ids),
          loop: () => clipLoop.open(target.regionId),
          loopText: clipLoop.text(target.regionId),
        }),
      });
    },
    [actions, clipLoop, ctrl, openClip, trigger],
  );

  const trackName = (id: string): string | null => snap.project.tracks.find((t) => t.id === id)?.name ?? null;

  const over = (e: DragEvent, target: DropTarget): void => {
    const payload = getDrag();
    if (!payload || !acceptDrop(payload, target, trackName).ok) {
      setHint(null);
      return;
    }
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
    setHint(target.kind === 'new-track' ? 'new-track' : target.kind === 'none' ? null : target.trackId);
  };

  const land = (e: DragEvent, target: DropTarget): void => {
    const payload = getDrag();
    setHint(null);
    if (!payload || payload.kind !== 'sound' || !acceptDrop(payload, target, trackName).ok) return;
    e.preventDefault();
    void dropSound({ actions, store }, payload, target);
  };

  const listTarget = (e: DragEvent): DropTarget => {
    const track = e.target instanceof HTMLElement ? trackOf(e.target) : null;
    return track ? { kind: 'track', trackId: track.id } : { kind: 'new-track' };
  };
  const laneTargetOf = (e: DragEvent): DropTarget => ctrl.current?.dropTargetAt(e.clientX, e.clientY) ?? { kind: 'none' };

  const leave = (e: DragEvent): void => {
    if (!(e.relatedTarget instanceof Node) || !e.currentTarget.contains(e.relatedTarget)) setHint(null);
  };

  const menu: ReactNode = (
    <>
      <ContextMenu request={trigger.request} onClose={trigger.close} />
      {clipLoop.dialog}
      <Dialog
        open={renaming !== null}
        onClose={() => setRenaming(null)}
        title="Rename track"
        onSubmit={() => {
          if (renaming) void actions.renameTrack(renaming.id, draft);
          setRenaming(null);
        }}
        primary={{ label: 'Rename', disabled: !draft.trim() }}
      >
        <label className="flex flex-col gap-1 text-sm text-ink-dim">
          Track name
          <input data-autofocus="" className={`${field} text-ink`} value={draft} autoComplete="off" onChange={(e) => setDraft(e.target.value)} />
        </label>
      </Dialog>
    </>
  );

  return {
    menu,
    onClipMenu,
    openTrackMenu,
    dragging,
    hint,
    trackAttr: (id: string) => ({ [TRACK_ATTR]: id }),
    listProps: {
      ...trigger.props,
      onDragOver: (e: DragEvent) => over(e, listTarget(e)),
      onDragLeave: leave,
      onDrop: (e: DragEvent) => land(e, listTarget(e)),
    },
    laneProps: {
      onDragOver: (e: DragEvent) => over(e, laneTargetOf(e)),
      onDragLeave: leave,
      onDrop: (e: DragEvent) => land(e, laneTargetOf(e)),
    },
  };
}
