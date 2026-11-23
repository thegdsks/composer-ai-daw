// Flat, windowed list behind the Sounds tab. Group and filter rules come from sounds/catalogue.ts
// (the same ones the Sound dock tab uses); this file only lays them out and picks the visible rows.
import { config } from '../config';
import type { SoundEntry } from '../backend/types';
import type { Browse, CategoryLabel } from '../sounds/catalogue';

export type LibraryItem =
  | { kind: 'group'; key: string; category: CategoryLabel; count: number; collapsed: boolean }
  | { kind: 'sound'; key: string; entry: SoundEntry }
  | { kind: 'pack'; key: string; entry: SoundEntry; reason: string };

export function itemHeight(item: LibraryItem): number {
  return item.kind === 'group' ? config.libraryHeaderHeight : config.libraryRowHeight;
}

/** Group headers with their sounds (unless collapsed), then sample packs last. */
export function flatten(result: Browse, collapsed: ReadonlySet<CategoryLabel>): LibraryItem[] {
  const out: LibraryItem[] = [];
  for (const g of result.groups) {
    const isCollapsed = collapsed.has(g.category);
    out.push({ kind: 'group', key: `group-${g.category}`, category: g.category, count: g.ready.length, collapsed: isCollapsed });
    if (!isCollapsed) for (const entry of g.ready) out.push({ kind: 'sound', key: entry.id, entry });
  }
  for (const p of result.packs) out.push({ kind: 'pack', key: p.entry.id, entry: p.entry, reason: p.reason });
  return out;
}

/** Top edge of each item, plus the total height as the last element. */
export function offsets(items: readonly LibraryItem[]): number[] {
  const out: number[] = [0];
  let y = 0;
  for (const it of items) {
    y += itemHeight(it);
    out.push(y);
  }
  return out;
}

/** Index range [first, last) to render for a scroll position, with a few extra rows each side. */
export function visibleRange(tops: readonly number[], scrollTop: number, viewHeight: number): { first: number; last: number } {
  const count = tops.length - 1;
  if (count <= 0) return { first: 0, last: 0 };
  let lo = 0;
  let hi = count;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if ((tops[mid + 1] ?? 0) <= scrollTop) lo = mid + 1;
    else hi = mid;
  }
  let end = lo;
  while (end < count && (tops[end] ?? 0) < scrollTop + viewHeight) end++;
  return {
    first: Math.max(0, lo - config.libraryOverscan),
    last: Math.min(count, end + config.libraryOverscan),
  };
}

/** Scroll position that brings item `index` fully into view, or the current one when it already is. */
export function scrollToShow(tops: readonly number[], index: number, scrollTop: number, viewHeight: number): number {
  const top = tops[index] ?? 0;
  const bottom = tops[index + 1] ?? top;
  if (top < scrollTop) return top;
  if (bottom > scrollTop + viewHeight) return Math.max(0, bottom - viewHeight);
  return scrollTop;
}

/** Next navigable index for an arrow, Home or End key, clamped to the list. */
export function stepIndex(current: number, key: 'up' | 'down' | 'home' | 'end', count: number): number {
  if (count <= 0) return -1;
  if (key === 'home') return 0;
  if (key === 'end') return count - 1;
  if (current < 0) return key === 'down' ? 0 : count - 1;
  return Math.min(count - 1, Math.max(0, current + (key === 'down' ? 1 : -1)));
}
