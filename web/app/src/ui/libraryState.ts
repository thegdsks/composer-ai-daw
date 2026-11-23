// View state of the library sidebar: open or rail, and its width. Browser-local, so storage may
// be blocked; every access is guarded. Nothing here is a project command.
import { config } from '../config';

export interface LibraryState {
  /** Open shows the list; closed leaves only the narrow rail. */
  open: boolean;
  /** Width in px while open. */
  width: number;
}

export function clampWidth(w: number): number {
  if (!Number.isFinite(w)) return config.libraryWidthDefault;
  return Math.round(Math.min(config.libraryWidthMax, Math.max(config.libraryWidthMin, w)));
}

/** First run (nothing stored, or damaged) opens the sidebar so a beginner sees sounds at once. */
export function parseLibrary(raw: string | null): LibraryState {
  const out: LibraryState = { open: true, width: clampWidth(config.libraryWidthDefault) };
  if (!raw) return out;
  try {
    const p = JSON.parse(raw) as Partial<LibraryState>;
    if (typeof p.open === 'boolean') out.open = p.open;
    if (typeof p.width === 'number') out.width = clampWidth(p.width);
  } catch {
    // Damaged value: keep the first run defaults.
  }
  return out;
}

function readStored(): string | null {
  try {
    return localStorage.getItem(config.libraryKey);
  } catch {
    return null;
  }
}

let current: LibraryState = parseLibrary(readStored());
const listeners = new Set<() => void>();

/** Width and rail size reach the layout as CSS variables, so no element carries a style attribute. */
function writeVars(): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  root.style.setProperty('--lib-w', `${current.width}px`);
  root.style.setProperty('--lib-rail', `${config.libraryRailWidth}px`);
}

export function getLibrary(): LibraryState {
  return current;
}

export function subscribeLibrary(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Pass `save: false` while a drag is in progress, then save once on release. */
export function setLibrary(patch: Partial<LibraryState>, save = true): void {
  const next = { ...current, ...patch };
  next.width = clampWidth(next.width);
  current = next;
  writeVars();
  if (save) {
    try {
      localStorage.setItem(config.libraryKey, JSON.stringify(current));
    } catch {
      // Not persisted; the change still applies to this session.
    }
  }
  for (const l of listeners) l();
}

export function toggleLibrary(): void {
  setLibrary({ open: !current.open });
}

/** Applies the stored size to the page. Call once before the first render. */
export function applyLibrary(): void {
  writeVars();
}

/** Test hook: reload the state from storage. */
export function reloadLibrary(): void {
  current = parseLibrary(readStored());
  for (const l of listeners) l();
}
