// History panel: every undo step from history.get, newest at the bottom, the current position
// marked. Clicking a step moves there in one call. Rows are windowed, so a long history costs
// only the rows in view, and entries are fetched a page at a time.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { HistoryEntry, StepSource } from '../backend/types';
import { config } from '../config';
import { XIcon } from '../icons';
import { plainLabel } from '../state/store';
import { iconBtn } from './classes';
import { useSession, useSnapshot } from './hooks';

const SOURCE_TEXT: Record<StepSource, string> = { user: 'You', ai: 'AI', template: 'Template', recipe: 'Recipe' };

/** Row 0 is the opening state; row n (n above 0) is history entry n minus 1. */
const EMPTY: ReadonlyMap<number, HistoryEntry> = new Map();

export function rowLabel(entry: HistoryEntry | undefined, row: number): string {
  if (row === 0) return 'Opening state';
  return entry ? plainLabel(entry.label.trim() || 'Edit') : 'Loading';
}

export function History(props: { open: boolean; onClose(): void }) {
  const { store } = useSession();
  const snap = useSnapshot();
  const cursor = snap.undoDepth;
  const rows = snap.undoDepth + snap.redoDepth + 1;
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);
  const [focusRow, setFocusRow] = useState(cursor);
  const [fetched, setFetched] = useState<{ key: string; map: ReadonlyMap<number, HistoryEntry> }>({ key: '', map: new Map() });
  const asked = useRef(new Set<number>());
  const version = useRef('');
  const rowPx = config.historyRowPx;

  const first = Math.max(0, Math.floor(scrollTop / rowPx) - config.historyOverscan);
  const last = Math.min(rows - 1, Math.ceil((scrollTop + height) / rowPx) + config.historyOverscan);

  // A new state hash invalidates every fetched page; the pages in view are fetched again below.
  const key = `${snap.stateHash}:${snap.undoDepth}:${snap.redoDepth}`;
  useEffect(() => {
    if (!props.open) return;
    if (version.current !== key) {
      version.current = key;
      asked.current = new Set();
    }
    const total = rows - 1;
    const page = config.historyPage;
    const needed = new Set<number>();
    for (let r = Math.max(1, first); r <= last; r++) if (r - 1 < total) needed.add(Math.floor((r - 1) / page));
    for (const p of needed) {
      if (asked.current.has(p)) continue;
      asked.current.add(p);
      void store.history(p * page, page).then((res) => {
        if (!res.ok || version.current !== key) return;
        setFetched((prev) => {
          const next = new Map(prev.key === key ? prev.map : []);
          for (const e of res.value.entries) next.set(e.index, e);
          return { key, map: next };
        });
      });
    }
  }, [props.open, key, first, last, rows, store]);

  // Pages fetched for an older state are ignored, so a stale label never shows.
  const entries = fetched.key === key ? fetched.map : EMPTY;

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el || !props.open) return;
    setHeight(el.clientHeight);
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, [props.open]);

  // Opening, or a jump from elsewhere, brings the current step into view and gives it the focus slot.
  useEffect(() => {
    const el = scroller.current;
    if (!props.open || !el) return;
    setFocusRow(cursor);
    const top = cursor * rowPx;
    if (top < el.scrollTop || top + rowPx > el.scrollTop + el.clientHeight) {
      el.scrollTop = Math.max(0, top - el.clientHeight / 2);
    }
  }, [props.open, cursor, rowPx]);

  const go = useCallback(
    (row: number) => {
      const label = rowLabel(entries.get(row - 1), row);
      void store.jumpTo(row, label);
    },
    [entries, store],
  );

  const moveFocus = useCallback(
    (row: number) => {
      const el = scroller.current;
      const next = Math.min(rows - 1, Math.max(0, row));
      setFocusRow(next);
      if (!el) return;
      const top = next * rowPx;
      if (top < el.scrollTop) el.scrollTop = top;
      else if (top + rowPx > el.scrollTop + el.clientHeight) el.scrollTop = top + rowPx - el.clientHeight;
      requestAnimationFrame(() => el.querySelector<HTMLElement>(`[data-row="${next}"]`)?.focus());
    },
    [rows, rowPx],
  );

  const onKey = (e: React.KeyboardEvent): void => {
    const page = Math.max(1, Math.floor(height / rowPx) - 1);
    const map: Record<string, number> = {
      ArrowUp: focusRow - 1,
      ArrowDown: focusRow + 1,
      Home: 0,
      End: rows - 1,
      PageUp: focusRow - page,
      PageDown: focusRow + page,
    };
    const target = map[e.key];
    if (target !== undefined) {
      e.preventDefault();
      moveFocus(target);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      props.onClose();
    }
  };

  // The rows outside the window are two spacers sized through custom properties (no inline style).
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.style.setProperty('--hist-lead', `${first * rowPx}px`);
    el.style.setProperty('--hist-trail', `${Math.max(0, rows - 1 - last) * rowPx}px`);
  }, [first, last, rows, rowPx, props.open]);

  const visible = useMemo(() => {
    const out: number[] = [];
    for (let r = first; r <= last; r++) out.push(r);
    return out;
  }, [first, last]);

  if (!props.open) return null;
  return (
    <aside
      id="history-panel"
      aria-label="History"
      data-testid="history-panel"
      className="fixed bottom-8 right-0 top-12 z-20 flex w-80 max-w-full flex-col border-l border-line bg-panel text-ink shadow-lg"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          props.onClose();
        }
      }}
    >
      <div className="flex items-center justify-between gap-2 border-b border-line px-3 py-2">
        <div>
          <h2 className="text-sm font-semibold">History</h2>
          <p className="text-xs text-ink-dim">Pick a step to go back or forward to it.</p>
        </div>
        <button type="button" className={iconBtn} aria-label="Close history" onClick={props.onClose}>
          <XIcon size={18} />
        </button>
      </div>
      <div
        ref={scroller}
        data-testid="history-list"
        className="relative min-h-0 flex-1 overflow-y-auto overscroll-contain"
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        onKeyDown={onKey}
      >
        <div className="h-(--hist-lead,0px)" aria-hidden="true" />
        <ol className="m-0 list-none p-0">
          {visible.map((row) => {
            const entry = row === 0 ? undefined : entries.get(row - 1);
            const label = rowLabel(entry, row);
            const here = row === cursor;
            const undone = row > cursor;
            return (
              <li key={row} className="h-9">
                <button
                  type="button"
                  data-row={row}
                  data-testid="history-step"
                  data-step={row}
                  aria-current={here ? 'step' : undefined}
                  tabIndex={row === focusRow ? 0 : -1}
                  className={`flex h-9 w-full items-center gap-2 px-3 text-left text-sm hover:bg-panel-2 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent ${
                    here ? 'bg-panel-2 font-semibold' : ''
                  } ${undone ? 'text-ink-dim' : ''}`}
                  onFocus={() => setFocusRow(row)}
                  onClick={() => go(row)}
                >
                  <span className="w-7 shrink-0 text-right text-xs tabular-nums text-ink-dim">{row}</span>
                  <span className="min-w-0 flex-1 truncate">
                    {label}
                    {entry && entry.commandCount > 1 && <span className="text-xs text-ink-dim">{` (${entry.commandCount} changes)`}</span>}
                  </span>
                  {entry && (
                    <span className="shrink-0 rounded border border-line px-1.5 text-xs">{SOURCE_TEXT[entry.source]}</span>
                  )}
                  {here && <span className="shrink-0 text-xs">You are here</span>}
                  {undone && <span className="shrink-0 text-xs">Undone</span>}
                </button>
              </li>
            );
          })}
        </ol>
        <div className="h-(--hist-trail,0px)" aria-hidden="true" />
      </div>
    </aside>
  );
}
