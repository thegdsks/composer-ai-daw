import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent,
  type PointerEvent,
  type RefObject,
} from 'react';
import type { SoundEntry } from '../backend/types';
import { config } from '../config';
import { ChevronIcon, FolderIcon, SidebarIcon, WaveIcon } from '../icons/library';
import { tip } from '../input/keymap';
import { browse, type CategoryLabel } from '../sounds/catalogue';
import { btnSmall, field, iconBtn } from './classes';
import { useSession, useSnapshot } from './hooks';
import { flatten, offsets, scrollToShow, stepIndex, visibleRange, type LibraryItem } from './libraryList';
import { clampWidth, getLibrary, setLibrary, subscribeLibrary, toggleLibrary } from './libraryState';
import { GroupRow, LicenceDetail, PackItem, SoundRow } from './SoundRow';
import { useSoundMenu } from './useSoundMenu';

const itemId = (key: string): string => `library-item-${key}`;

/** Vertical divider on the right edge of the sidebar: drag, arrow keys, Enter resets. */
function LibrarySplitter(props: { width: number; asideRef: RefObject<HTMLElement | null> }) {
  const dragging = useRef(false);

  const move = (clientX: number, save: boolean): void => {
    const left = props.asideRef.current?.getBoundingClientRect().left ?? 0;
    setLibrary({ width: clampWidth(clientX - left) }, save);
  };
  const onDown = (e: PointerEvent<HTMLDivElement>): void => {
    if (e.button !== 0) return;
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    e.preventDefault();
  };
  const onUp = (e: PointerEvent<HTMLDivElement>): void => {
    if (!dragging.current) return;
    dragging.current = false;
    move(e.clientX, true);
  };
  const onKey = (e: KeyboardEvent<HTMLDivElement>): void => {
    let next = props.width;
    if (e.key === 'ArrowLeft') next -= config.libraryKeyStep;
    else if (e.key === 'ArrowRight') next += config.libraryKeyStep;
    else if (e.key === 'Home') next = config.libraryWidthMin;
    else if (e.key === 'End') next = config.libraryWidthMax;
    else if (e.key === 'Enter') next = config.libraryWidthDefault;
    else return;
    e.preventDefault();
    setLibrary({ width: clampWidth(next) });
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize the library. Arrow keys move it, Enter resets it."
      aria-valuemin={config.libraryWidthMin}
      aria-valuemax={config.libraryWidthMax}
      aria-valuenow={props.width}
      tabIndex={0}
      data-testid="library-splitter"
      className="group relative z-10 w-1.5 shrink-0 cursor-col-resize touch-none bg-line max-sm:hidden hover:bg-accent focus-visible:bg-accent"
      onPointerDown={onDown}
      onPointerMove={(e) => {
        if (dragging.current) move(e.clientX, false);
      }}
      onPointerUp={onUp}
      onPointerCancel={onUp}
      onKeyDown={onKey}
      onDoubleClick={() => setLibrary({ width: config.libraryWidthDefault })}
    >
      {/* A wider invisible target makes the thin line easy to grab. */}
      <span className="absolute inset-y-0 -left-1.5 -right-1.5" aria-hidden="true" />
    </div>
  );
}

/** Rows in view only; spacers above and below keep the scroll height right. */
function useWindow(tops: readonly number[]) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [view, setView] = useState({ top: 0, height: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = (): void => setView({ top: el.scrollTop, height: el.clientHeight });
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    el.addEventListener('scroll', read, { passive: true });
    return () => {
      ro.disconnect();
      el.removeEventListener('scroll', read);
    };
  }, []);
  const { first, last } = visibleRange(tops, view.top, view.height);
  const count = tops.length - 1;
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const total = tops[count] ?? 0;
    el.style.setProperty('--lib-lead', `${tops[first] ?? 0}px`);
    el.style.setProperty('--lib-trail', `${Math.max(0, total - (tops[last] ?? total))}px`);
  }, [first, last, tops, count]);
  return { ref, first, last };
}

function SoundsPanel() {
  const { actions, audio, store, sounds } = useSession();
  const snap = useSnapshot();
  const [query, setQuery] = useState('');
  const [collapsed, setCollapsed] = useState<ReadonlySet<CategoryLabel>>(new Set());
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const searchId = useId();
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const entries = sounds.catalogue?.entries;
  const items = useMemo(() => flatten(browse(entries ?? [], query, null), collapsed), [entries, query, collapsed]);
  const tops = useMemo(() => offsets(items), [items]);
  const { ref, first, last } = useWindow(tops);
  const track = snap.project.tracks.find((t) => t.id === snap.selectedTrack) ?? null;
  const inUse = useMemo(() => new Set(snap.project.tracks.map((t) => t.sound)), [snap.project.tracks]);

  const navigable = useMemo(() => items.filter((i) => i.kind !== 'pack'), [items]);
  const activeItem = items.find((i) => i.key === activeKey) ?? null;
  const activeSound = activeItem?.kind === 'sound' ? activeItem.entry : null;
  const soundMenu = useSoundMenu(entries, activeSound);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const audition = useCallback(
    (e: SoundEntry, delay: number): void => {
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => void audio.preview(e.id), delay);
    },
    [audio],
  );

  const toggleGroup = useCallback((category: CategoryLabel, open?: boolean): void => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      const wantOpen = open ?? next.has(category);
      if (wantOpen) next.delete(category);
      else next.add(category);
      return next;
    });
  }, []);

  /** Gives the sound to the selected track; with no track it adds one first so nothing is a dead end. */
  const assignSound = useCallback(
    async (e: SoundEntry): Promise<void> => {
      let target = store.getSnapshot().selectedTrack;
      if (!store.getSnapshot().project.tracks.some((t) => t.id === target)) {
        await actions.addTrack();
        target = store.getSnapshot().selectedTrack;
      }
      if (!target) return;
      if (await actions.setSound(target, e.id)) void audio.preview(e.id);
    },
    [actions, audio, store],
  );

  const move = (index: number, preview: boolean): void => {
    const item = navigable[index];
    if (!item) return;
    setActiveKey(item.key);
    const at = items.indexOf(item);
    const el = ref.current;
    if (el) el.scrollTop = scrollToShow(tops, at, el.scrollTop, el.clientHeight);
    if (preview && item.kind === 'sound') audition(item.entry, config.libraryAuditionDelayMs);
  };

  const onKey = (e: KeyboardEvent<HTMLDivElement>): void => {
    soundMenu.props.onKeyDown(e);
    if (e.defaultPrevented) return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const cur = navigable.findIndex((i) => i.key === activeKey);
    const keys: Record<string, 'up' | 'down' | 'home' | 'end'> = {
      ArrowUp: 'up',
      ArrowDown: 'down',
      Home: 'home',
      End: 'end',
    };
    const step = keys[e.key];
    if (step) {
      e.preventDefault();
      move(stepIndex(cur, step, navigable.length), true);
    } else if (e.key === 'Enter' && activeItem) {
      e.preventDefault();
      if (activeItem.kind === 'sound') void assignSound(activeItem.entry);
      else if (activeItem.kind === 'group') toggleGroup(activeItem.category);
    } else if (e.key === 'ArrowRight' && activeItem?.kind === 'group' && activeItem.collapsed) {
      e.preventDefault();
      toggleGroup(activeItem.category, true);
    } else if (e.key === 'ArrowLeft' && activeItem) {
      e.preventDefault();
      if (activeItem.kind === 'group' && !activeItem.collapsed) toggleGroup(activeItem.category, false);
      else if (activeItem.kind === 'sound') {
        const owner = items
          .slice(0, items.indexOf(activeItem))
          .reverse()
          .find((i) => i.kind === 'group');
        if (owner) move(navigable.indexOf(owner), false);
      }
    }
  };

  if (!sounds.catalogue) {
    return (
      <p className="m-0 p-3 text-sm text-ink-dim" data-testid="library-unavailable">
        {sounds.error ?? 'The sound list is not available. Reload the page.'}
      </p>
    );
  }

  const rows: LibraryItem[] = items.slice(first, last);
  return (
    <div className="flex min-h-0 flex-1 flex-col" id="library-panel-sounds">
      <div className="shrink-0 p-2">
        <label htmlFor={searchId} className="sr-only">
          Search the library
        </label>
        <input
          id={searchId}
          type="search"
          autoComplete="off"
          placeholder="Search the library"
          className={`${field} w-full`}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') {
              e.preventDefault();
              ref.current?.focus();
              if (!activeKey) move(0, false);
            }
          }}
        />
      </div>
      <div
        ref={ref}
        role="tree"
        aria-label="Sounds"
        tabIndex={0}
        aria-activedescendant={activeKey && rows.some((r) => r.key === activeKey) ? itemId(activeKey) : undefined}
        data-testid="library-list"
        data-space-play
        className="min-h-0 flex-1 overflow-y-auto border-y border-line outline-offset-[-2px]"
        onKeyDown={onKey}
        onContextMenu={soundMenu.props.onContextMenu}
      >
        {items.length === 0 && (
          <p className="m-0 p-3 text-sm text-ink-dim" data-testid="library-empty">
            No sound matches. Try a shorter word.
          </p>
        )}
        <div className="h-(--lib-lead,0px)" aria-hidden="true" />
        {rows.map((it) => {
          const id = itemId(it.key);
          if (it.kind === 'group')
            return (
              <GroupRow
                key={it.key}
                id={id}
                category={it.category}
                count={it.count}
                collapsed={it.collapsed}
                active={activeKey === it.key}
                onToggle={() => {
                  setActiveKey(it.key);
                  toggleGroup(it.category);
                }}
              />
            );
          if (it.kind === 'pack') return <PackItem key={it.key} id={id} entry={it.entry} reason={it.reason} />;
          return (
            <SoundRow
              key={it.key}
              id={id}
              entry={it.entry}
              active={activeKey === it.key}
              inUse={inUse.has(it.entry.id)}
              onPick={() => {
                setActiveKey(it.key);
                if (timer.current) clearTimeout(timer.current);
                void audio.preview(it.entry.id);
              }}
              onUse={() => void assignSound(it.entry)}
            />
          );
        })}
        <div className="h-(--lib-trail,0px)" aria-hidden="true" />
      </div>
      {soundMenu.element}
      <div className="flex shrink-0 flex-col gap-1.5 p-2" data-testid="library-footer">
        {activeSound ? (
          <>
            <LicenceDetail entry={activeSound} />
            <button
              type="button"
              className={`${btnSmall} self-start`}
              data-testid="library-use"
              onClick={() => void assignSound(activeSound)}
            >
              {track ? `Use on ${track.name}` : 'Add a track and use'}
            </button>
          </>
        ) : (
          <p className="m-0 text-xs text-ink-dim">Click a sound to hear it. Double click or press Enter to use it.</p>
        )}
      </div>
    </div>
  );
}

/** Left sidebar: open it for the Sounds list, or collapse it to a narrow rail (Shift+B). */
export function Library() {
  const lib = useSyncExternalStore(subscribeLibrary, getLibrary);
  const asideRef = useRef<HTMLElement | null>(null);

  if (!lib.open) {
    return (
      <aside
        aria-label="Library"
        data-testid="library"
        data-state="rail"
        className="flex w-(--lib-rail) shrink-0 flex-col items-center gap-1 border-r border-line bg-panel py-2 max-sm:hidden"
      >
        <button
          type="button"
          className={iconBtn}
          aria-label="Show the library"
          aria-expanded={false}
          title={tip('Show the library', 'library')}
          data-testid="library-toggle"
          onClick={toggleLibrary}
        >
          <SidebarIcon size={18} />
        </button>
        <button type="button" className={iconBtn} aria-label="Show the sounds" title="Sounds" onClick={toggleLibrary}>
          <WaveIcon size={18} />
        </button>
      </aside>
    );
  }

  return (
    <>
      <aside
        ref={asideRef}
        aria-label="Library"
        data-testid="library"
        data-state="open"
        className="flex w-(--lib-w) shrink-0 flex-col bg-panel max-sm:hidden"
      >
        <div className="flex shrink-0 items-center border-b border-line px-1">
          <div role="group" aria-label="Library scopes" className="flex flex-1 items-end">
            <button
              type="button"
              id="library-tab-sounds"
              aria-pressed
              aria-controls="library-panel-sounds"
              className="inline-flex h-9 items-center gap-1.5 border-b-2 border-accent px-2 text-sm font-medium text-ink"
            >
              <WaveIcon size={16} />
              Sounds
            </button>
            <button
              type="button"
              id="library-tab-samples"
              aria-pressed={false}
              disabled
              title="Samples need audio import, which is not built yet"
              className="inline-flex h-9 items-center gap-1.5 border-b-2 border-transparent px-2 text-sm font-medium text-ink-dim disabled:cursor-not-allowed disabled:opacity-60"
            >
              <FolderIcon size={16} />
              Samples
            </button>
          </div>
          <button
            type="button"
            className={iconBtn}
            aria-label="Hide the library"
            aria-expanded
            aria-controls="library-body"
            title={tip('Hide the library', 'library')}
            data-testid="library-toggle"
            onClick={toggleLibrary}
          >
            <ChevronIcon size={16} className="rotate-180" />
          </button>
        </div>
        <div id="library-body" className="flex min-h-0 flex-1 flex-col">
          <SoundsPanel />
        </div>
      </aside>
      <LibrarySplitter width={lib.width} asideRef={asideRef} />
    </>
  );
}
