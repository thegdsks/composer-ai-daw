import { useCallback, type KeyboardEvent, type MouseEvent } from 'react';
import type { SoundEntry } from '../backend/types';
import { dropSound } from '../state/soundDrop';
import { ContextMenu, useMenuTrigger } from './ContextMenu';
import { useSession } from './hooks';
import { isMenuKey } from './menuModel';
import { soundMenuItems } from './menus';

/**
 * The keyboard and menu twin of dragging a sound: Shift+F10 or right click on a sound lists every
 * track plus "new track", and choosing one does exactly what the drop would.
 */
export function useSoundMenu(entries: readonly SoundEntry[] | undefined, active: SoundEntry | null) {
  const { actions, store } = useSession();

  const build = useCallback(
    (origin: HTMLElement) => {
      const id = origin.closest<HTMLElement>('[data-sound]')?.dataset.sound;
      const entry = entries?.find((e) => e.id === id) ?? active;
      if (!entry) return null;
      return {
        label: `Use ${entry.name}`,
        items: soundMenuItems(entry, store.getSnapshot().project.tracks, (target) =>
          void dropSound({ actions, store }, { soundId: entry.id, name: entry.name }, target),
        ),
      };
    },
    [entries, active, actions, store],
  );
  const trigger = useMenuTrigger(build);

  return {
    element: <ContextMenu request={trigger.request} onClose={trigger.close} />,
    props: {
      onContextMenu: (e: MouseEvent<HTMLElement>) => trigger.props.onContextMenu(e),
      onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
        if (isMenuKey(e) && active) trigger.props.onKeyDown(e);
      },
    },
  };
}
