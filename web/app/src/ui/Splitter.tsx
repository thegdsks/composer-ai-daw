import { useRef, useSyncExternalStore, type KeyboardEvent, type PointerEvent, type RefObject } from 'react';
import { config } from '../config';
import { effectiveSplit, getPrefs, setPrefs, subscribePrefs, trackRowHeight } from '../prefs';

/** Smallest and largest arrangement share, in percent of the workspace, given its pixel height. */
function limits(height: number): { min: number; max: number } {
  if (height <= 0) return { min: 0, max: 100 };
  const minPx = config.trackHeaderHeight + trackRowHeight() * config.minArrangementRows;
  const min = (minPx / height) * 100;
  const max = 100 - (config.minDockHeight / height) * 100;
  return { min, max: Math.max(min, max) };
}

const clampPct = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

/** Draggable and keyboard operable divider between the arrangement and the dock. */
export function Splitter({ area }: { area: RefObject<HTMLElement | null> }) {
  const prefs = useSyncExternalStore(subscribePrefs, getPrefs);
  const dragging = useRef(false);

  const move = (clientY: number, save: boolean): void => {
    const el = area.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const { min, max } = limits(rect.height);
    const pct = clampPct(((clientY - rect.top) / rect.height) * 100, min, max);
    setPrefs({ split: Math.round(pct * 10) / 10, splitUser: true }, save);
  };

  const onDown = (e: PointerEvent<HTMLDivElement>): void => {
    if (e.button !== 0) return;
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    e.preventDefault();
  };
  const onMove = (e: PointerEvent<HTMLDivElement>): void => {
    if (dragging.current) move(e.clientY, false);
  };
  const onUp = (e: PointerEvent<HTMLDivElement>): void => {
    if (!dragging.current) return;
    dragging.current = false;
    move(e.clientY, true);
  };

  const onKey = (e: KeyboardEvent<HTMLDivElement>): void => {
    const el = area.current;
    const { min, max } = limits(el?.getBoundingClientRect().height ?? 0);
    let next = effectiveSplit();
    if (e.key === 'ArrowUp') next -= config.splitKeyStepPercent;
    else if (e.key === 'ArrowDown') next += config.splitKeyStepPercent;
    else if (e.key === 'Home') next = min;
    else if (e.key === 'End') next = max;
    else if (e.key === 'Enter') {
      e.preventDefault();
      setPrefs({ splitUser: false });
      return;
    } else return;
    e.preventDefault();
    setPrefs({ split: Math.round(clampPct(next, min, max) * 10) / 10, splitUser: true });
  };

  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize the arrangement and the dock. Arrow keys move it, Enter resets it."
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(prefs.splitUser ? prefs.split : effectiveSplit())}
      tabIndex={0}
      data-testid="splitter"
      className="group relative z-10 h-1.5 shrink-0 cursor-row-resize touch-none bg-line hover:bg-accent focus-visible:bg-accent max-sm:hidden"
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={onUp}
      onKeyDown={onKey}
      onDoubleClick={() => setPrefs({ splitUser: false })}
    >
      {/* A taller invisible target makes the thin line easy to grab. */}
      <span className="absolute inset-x-0 -top-1.5 -bottom-1.5" aria-hidden="true" />
    </div>
  );
}
