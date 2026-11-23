// Browser-local preferences: theme, density and the workspace split. Storage may be blocked
// or full, so every access is guarded and the app still works for the session.
import { config } from './config';

export type Theme = 'system' | 'light' | 'dark';
export type Density = 'comfortable' | 'compact';

export interface Prefs {
  theme: Theme;
  density: Density;
  /** Arrangement share of the workspace height, percent. */
  split: number;
  /** Start screen: shown on first open, then 'seen', or 'never' when the user opted out. */
  start: 'show' | 'seen' | 'never';
  /** The first run card was dismissed. */
  firstRunDone: boolean;
  /** Times the next action chip was dismissed. */
  chipDismissals: number;
  /** The computer keyboard plays notes (K). Off until asked, so letter shortcuts stay free. */
  pianoKeys: boolean;
  /** Plain toasts shown; toasts with an Undo button always show. */
  toasts: boolean;
  /** The chord strip above the piano roll is open. */
  chordStrip: boolean;
  /** Loudness lane under the piano roll: auto folds it on a short roll, open and closed are the user's choice. */
  velocityLane: 'auto' | 'open' | 'closed';
  /** The split was dragged or keyed, so it no longer follows the open dock tab. */
  splitUser: boolean;
}

const THEMES: readonly Theme[] = ['system', 'light', 'dark'];
const DENSITIES: readonly Density[] = ['comfortable', 'compact'];

function defaults(): Prefs {
  return { theme: 'system', density: 'comfortable', split: config.splitDefaultPercent, start: 'show', firstRunDone: false, chipDismissals: 0, pianoKeys: false, toasts: true, chordStrip: false, velocityLane: 'auto', splitUser: false };
}

function read(): Prefs {
  const out = defaults();
  try {
    const raw = localStorage.getItem(config.prefsKey);
    if (!raw) return out;
    const p = JSON.parse(raw) as Partial<Prefs>;
    if (p.theme && THEMES.includes(p.theme)) out.theme = p.theme;
    if (p.density && DENSITIES.includes(p.density)) out.density = p.density;
    if (typeof p.split === 'number' && Number.isFinite(p.split)) out.split = p.split;
    if (p.start === 'show' || p.start === 'seen' || p.start === 'never') out.start = p.start;
    if (typeof p.pianoKeys === 'boolean') out.pianoKeys = p.pianoKeys;
    if (typeof p.firstRunDone === 'boolean') out.firstRunDone = p.firstRunDone;
    if (typeof p.chipDismissals === 'number' && Number.isFinite(p.chipDismissals)) out.chipDismissals = Math.max(0, Math.floor(p.chipDismissals));
    if (typeof p.toasts === 'boolean') out.toasts = p.toasts;
    if (typeof p.chordStrip === 'boolean') out.chordStrip = p.chordStrip;
    if (p.velocityLane === 'auto' || p.velocityLane === 'open' || p.velocityLane === 'closed') out.velocityLane = p.velocityLane;
    if (typeof p.splitUser === 'boolean') out.splitUser = p.splitUser;
  } catch {
    // Storage blocked or the value is damaged: keep the defaults.
  }
  return out;
}

let current: Prefs = read();
const listeners = new Set<() => void>();
let sheet: CSSStyleSheet | null = null;
let notesTab = true;

/** Arrangement share in force: the user's split once chosen, else a taller dock while Notes is open. */
export function effectiveSplit(): number {
  if (current.splitUser) return current.split;
  return notesTab ? config.splitNotesPercent : config.splitDefaultPercent;
}

/** The dock tells prefs which tab is open so the default split can favour the roll. */
export function setNotesTab(open: boolean): void {
  if (notesTab === open) return;
  notesTab = open;
  writeDom();
  for (const l of listeners) l();
}

/** Split and lane limits are CSS variables in a constructed sheet, so no element carries a style attribute. */
function writeVars(): void {
  const rows = current.density === 'compact' ? config.trackRowHeightCompact : config.trackRowHeight;
  const armin = config.trackHeaderHeight + rows * config.minArrangementRows;
  const css = `:root{--arr-min:${armin}px;--dock-min:${config.minDockHeight}px;--split:${effectiveSplit()}%}`;
  try {
    sheet ??= new CSSStyleSheet();
    sheet.replaceSync(css);
    if (!document.adoptedStyleSheets.includes(sheet)) document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  } catch {
    // Browsers without constructable sheets fall back to custom properties on the root.
    const root = document.documentElement;
    root.style.setProperty('--arr-min', `${armin}px`);
    root.style.setProperty('--dock-min', `${config.minDockHeight}px`);
    root.style.setProperty('--split', `${effectiveSplit()}%`);
  }
}

function writeDom(): void {
  const root = document.documentElement;
  if (current.theme === 'system') delete root.dataset.theme;
  else root.dataset.theme = current.theme;
  root.dataset.density = current.density;
  writeVars();
}

function persist(): void {
  try {
    localStorage.setItem(config.prefsKey, JSON.stringify(current));
  } catch {
    // Not persisted; the change still applies to this session.
  }
}

export function getPrefs(): Prefs {
  return current;
}

export function subscribePrefs(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** Pass `save: false` while a drag is in progress, then save once on release. */
export function setPrefs(patch: Partial<Prefs>, save = true): void {
  current = { ...current, ...patch };
  writeDom();
  if (save) persist();
  for (const l of listeners) l();
}

/** Height of one track lane for the current density, shared by the DOM list and the canvas. */
export function trackRowHeight(): number {
  return current.density === 'compact' ? config.trackRowHeightCompact : config.trackRowHeight;
}

/** Applies stored preferences to the page. Call once before the first render. */
export function applyPrefs(): void {
  writeDom();
}
