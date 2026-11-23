// Track color as view state: the family comes from the sound category, a manual pick overrides it.
// Not a command, so it is not in the project file and not in undo; it is remembered in this browser.
import type { SoundEntry } from '../backend/types';
import type { Id, Track } from '../model';
import { categoryOf, type CategoryLabel } from '../sounds/catalogue';

export const COLOR_FAMILIES = ['keys', 'strings', 'pads', 'bass', 'plucks', 'choir', 'bells', 'perc'] as const;
export type ColorFamily = (typeof COLOR_FAMILIES)[number];

const STORAGE_KEY = 'composer.trackview.v1';

/** Full class names so Tailwind can see them; each reads a family token from index.css. */
export const FAMILY_CLASS: Record<ColorFamily, string> = {
  keys: 'bg-family-keys',
  strings: 'bg-family-strings',
  pads: 'bg-family-pads',
  bass: 'bg-family-bass',
  plucks: 'bg-family-plucks',
  choir: 'bg-family-choir',
  bells: 'bg-family-bells',
  perc: 'bg-family-perc',
};

const CATEGORY_FAMILY: Record<CategoryLabel, ColorFamily> = {
  Keys: 'keys',
  Strings: 'strings',
  Pads: 'pads',
  Bass: 'bass',
  Plucks: 'plucks',
  Choir: 'choir',
  Bells: 'bells',
  Pulse: 'perc',
  More: 'perc',
};

export function familyOfCategory(category: CategoryLabel): ColorFamily {
  return CATEGORY_FAMILY[category];
}

/** The family a track shows: its override, else its sound's category, else keys. */
export function familyOfTrack(track: Track, entries: readonly SoundEntry[], override: ColorFamily | undefined): ColorFamily {
  if (override) return override;
  const entry = track.sound ? entries.find((e) => e.id === track.sound) : undefined;
  return entry ? familyOfCategory(categoryOf(entry)) : 'keys';
}

/** Next pick when the chip is pressed: through the families, then back to automatic (undefined). */
export function nextOverride(current: ColorFamily | undefined): ColorFamily | undefined {
  if (current === undefined) return COLOR_FAMILIES[0];
  const at = COLOR_FAMILIES.indexOf(current);
  return COLOR_FAMILIES[at + 1];
}

function isFamily(v: unknown): v is ColorFamily {
  return typeof v === 'string' && (COLOR_FAMILIES as readonly string[]).includes(v);
}

export function parseOverrides(raw: string | null): Record<Id, ColorFamily> {
  if (!raw) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== 'object' || parsed === null) return {};
    const out: Record<Id, ColorFamily> = {};
    for (const [id, v] of Object.entries(parsed)) if (isFamily(v)) out[id] = v;
    return out;
  } catch {
    return {};
  }
}

let overrides: Record<Id, ColorFamily> = {};
let loaded = false;
const listeners = new Set<() => void>();

function load(): void {
  if (loaded) return;
  loaded = true;
  try {
    overrides = parseOverrides(localStorage.getItem(STORAGE_KEY));
  } catch {
    overrides = {};
  }
}

export function getOverrides(): Record<Id, ColorFamily> {
  load();
  return overrides;
}

export function subscribeOverrides(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}

export function setOverride(track: Id, family: ColorFamily | undefined): void {
  load();
  const next = { ...overrides };
  if (family) next[track] = family;
  else delete next[track];
  overrides = next;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage can be full or blocked; the pick still holds for this visit.
  }
  for (const l of listeners) l();
}
