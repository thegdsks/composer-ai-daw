// Context menu rules kept out of React so they can be tested: item shape and keyboard movement.

export interface MenuItem {
  id: string;
  label: string;
  /** Shortcut text shown at the right edge, for example "Ctrl D". */
  shortcut?: string;
  /** Disabled items stay focusable and say why. */
  disabled?: boolean;
  reason?: string;
  /** One line under the label that says what the item does. */
  hint?: string;
  danger?: boolean;
  /** A thin line above this item. */
  separated?: boolean;
  run(): void;
}

/**
 * Where focus goes for a key: arrows wrap, Home and End jump, a letter jumps to the next label that
 * starts with it. Returns null when the key is not a movement key.
 */
export function menuTarget(labels: readonly string[], current: number, key: string): number | null {
  const n = labels.length;
  if (n === 0) return null;
  if (key === 'ArrowDown') return (current + 1) % n;
  if (key === 'ArrowUp') return (current <= 0 ? n : current) - 1;
  if (key === 'Home') return 0;
  if (key === 'End') return n - 1;
  if (key.length === 1 && /\S/.test(key)) {
    const k = key.toLowerCase();
    for (let i = 1; i <= n; i++) {
      const at = (current + i) % n;
      if (labels[at]?.toLowerCase().startsWith(k)) return at;
    }
  }
  return null;
}

/** Keeps a menu of the given size inside the viewport, preferring the requested corner. */
export function placeMenu(
  at: { x: number; y: number },
  size: { w: number; h: number },
  view: { w: number; h: number },
  margin: number,
): { x: number; y: number } {
  return {
    x: Math.max(margin, Math.min(at.x, view.w - size.w - margin)),
    y: Math.max(margin, Math.min(at.y, view.h - size.h - margin)),
  };
}

/** True for the keys that open a context menu from the keyboard. */
export const isMenuKey = (e: { key: string; shiftKey: boolean; ctrlKey: boolean; metaKey: boolean; altKey: boolean }): boolean =>
  e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey);
