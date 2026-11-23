// Reorder of track headers by pointer drag on the grip and by keyboard. Every move is one move_track.
import { useCallback, useEffect, useRef, useState, type HTMLAttributes, type KeyboardEvent, type PointerEvent, type RefObject } from 'react';
import type { Id } from '../model';
import type { Store } from '../state/store';
import { moveTrackTo } from '../state/trackActions';
import { reorderTarget, slotAtY, stepTarget } from '../state/trackPlans';

interface Drag {
  id: Id;
  from: number;
  over: number;
}

function rowTops(list: HTMLElement | null): number[] {
  return Array.from(list?.querySelectorAll<HTMLElement>('[data-track-id]') ?? []).map((el) => el.getBoundingClientRect().top);
}

export function useTrackReorder(list: RefObject<HTMLElement | null>, store: Store, order: readonly Id[]) {
  const [drag, setDrag] = useState<Drag | null>(null);
  const refocus = useRef<Id | null>(null);

  // A moved row is re-inserted by the browser, which drops focus; put it back on the same grip.
  useEffect(() => {
    const id = refocus.current;
    if (!id) return;
    refocus.current = null;
    list.current?.querySelector<HTMLElement>(`[data-track-id="${id}"] [data-grip]`)?.focus({ preventScroll: true });
  }, [order, list]);

  const say = useCallback(
    (id: Id) => {
      const tracks = store.getSnapshot().project.tracks;
      const at = tracks.findIndex((t) => t.id === id);
      if (at >= 0) store.say(`${tracks[at]?.name ?? 'Track'} moved to position ${at + 1} of ${tracks.length}`);
    },
    [store],
  );

  const move = useCallback(
    async (id: Id, slot: number): Promise<void> => {
      refocus.current = id;
      if (await moveTrackTo(store, id, slot)) say(id);
      else refocus.current = null;
    },
    [store, say],
  );

  const keyProps = (id: Id) => (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === 'Escape' && drag) {
      setDrag(null);
      return;
    }
    const from = order.indexOf(id);
    const map: Record<string, -1 | 1 | 'start' | 'end'> = { ArrowUp: -1, ArrowDown: 1, Home: 'start', End: 'end' };
    const step = map[e.key];
    if (step === undefined) return;
    e.preventDefault();
    const to = stepTarget(from, step, order.length);
    if (to !== null) void move(id, to);
  };

  const pointerProps = (id: Id): HTMLAttributes<HTMLButtonElement> => ({
    onPointerDown(e: PointerEvent<HTMLButtonElement>) {
      if (e.button !== 0) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      const from = order.indexOf(id);
      setDrag({ id, from, over: from });
    },
    onPointerMove(e: PointerEvent<HTMLButtonElement>) {
      if (!drag || drag.id !== id) return;
      const over = slotAtY(e.clientY, rowTops(list.current));
      if (over !== drag.over) setDrag({ ...drag, over });
    },
    onPointerUp() {
      if (!drag || drag.id !== id) return;
      const to = reorderTarget(drag.from, drag.over, order.length);
      setDrag(null);
      if (to !== null) void move(id, to);
    },
    onPointerCancel() {
      setDrag(null);
    },
    onLostPointerCapture() {
      setDrag(null);
    },
  });

  /** For the row: where the drop indicator sits, if this row is the target. */
  const indicator = (index: number): 'up' | 'down' | undefined => {
    if (!drag || drag.over !== index || drag.over === drag.from) return undefined;
    return drag.over < drag.from ? 'up' : 'down';
  };

  return { dragging: drag?.id ?? null, keyProps, pointerProps, indicator };
}
