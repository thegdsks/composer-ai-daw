// Sound browser data rules: categories, which entries can be played, which are hidden.
// Pure functions over catalogue entries; the UI only renders what they return.
import type { SoundEntry } from '../backend/types';

export const CATEGORIES = ['Keys', 'Strings', 'Pads', 'Bass', 'Plucks', 'Choir', 'Bells', 'Pulse'] as const;
export type Category = (typeof CATEGORIES)[number];

/** Plain label for entries that fit no category (sample packs only). */
export const OTHER_CATEGORY = 'More';
export type CategoryLabel = Category | typeof OTHER_CATEGORY;

/** Built in voices, by catalogue id (composer-engine, D29). */
const BUILTIN_CATEGORY: Record<string, Category> = {
  'builtin-soft-piano': 'Keys',
  'builtin-warm-pad': 'Pads',
  'builtin-warm-strings': 'Strings',
  'builtin-pluck': 'Plucks',
  'builtin-sub-bass': 'Bass',
  'builtin-choir-pad': 'Choir',
  'builtin-bell': 'Bells',
  'builtin-soft-pulse': 'Pulse',
  'builtin-electric-piano': 'Keys',
  'builtin-mallet': 'Bells',
  'builtin-guitar-pluck': 'Plucks',
  'builtin-brass-pad': 'Pads',
  'builtin-flute': 'Pulse',
  'builtin-organ': 'Keys',
  'builtin-bass-pluck': 'Bass',
  'builtin-lead': 'Pulse',
};

/** Word hints for sample packs, checked in order against name and description. */
const HINTS: [RegExp, Category][] = [
  [/choir|vocal|voice/i, 'Choir'],
  [/bell|chime|glock|celesta|xylophone|marimba|mallet/i, 'Bells'],
  [/string|violin|cello|viola|bowed/i, 'Strings'],
  [/pad|sweep|new age|brass/i, 'Pads'],
  [/bass/i, 'Bass'],
  [/pluck|harp|guitar|pizz|ukulele/i, 'Plucks'],
  [/pulse|arp/i, 'Pulse'],
  [/piano|organ|keys|rhodes|harpsichord/i, 'Keys'],
];

export function categoryOf(e: SoundEntry): CategoryLabel {
  const fixed = BUILTIN_CATEGORY[e.id];
  if (fixed) return fixed;
  const text = `${e.name} ${e.instruments}`;
  for (const [re, cat] of HINTS) if (re.test(text)) return cat;
  return OTHER_CATEGORY;
}

const SOUND_ID = /^[a-z0-9-]{1,64}$/;

export type Availability =
  | { state: 'ready' }
  | { state: 'unavailable'; reason: string }
  | { state: 'hidden' };

export const PACK_REASON = 'Sample packs cannot load in the browser yet.';

/**
 * `ready`: the engine can play it now. `unavailable`: a sample pack, shown with a plain
 * reason because the browser cannot load banks yet. `hidden`: nothing playable ever
 * (not allowed to ship, a plugin, or loose per sound downloads).
 */
export function availability(e: SoundEntry): Availability {
  if (e.kind === 'builtin') return SOUND_ID.test(e.id) ? { state: 'ready' } : { state: 'hidden' };
  if (e.bundle === 'no') return { state: 'hidden' };
  if (e.kind === 'soundfont' && e.status === 'wired' && e.file) return SOUND_ID.test(e.id) ? { state: 'ready' } : { state: 'hidden' };
  if (e.kind === 'sfz' || e.kind === 'soundfont') return { state: 'unavailable', reason: PACK_REASON };
  return { state: 'hidden' };
}

export interface SoundGroup {
  category: CategoryLabel;
  ready: SoundEntry[];
}

export interface Browse {
  /** Playable sounds grouped by category, in category order. */
  groups: SoundGroup[];
  /** Sample packs that match, shown disabled with a reason. */
  packs: { entry: SoundEntry; reason: string }[];
  /** Playable sounds that match. */
  readyCount: number;
}

function matches(e: SoundEntry, query: string): boolean {
  if (!query) return true;
  const hay = `${e.name} ${e.instruments} ${categoryOf(e)}`.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

/** Filters by search text and category (null means all) and applies the hidden rule. */
export function browse(entries: readonly SoundEntry[], query: string, category: Category | null): Browse {
  const groups = new Map<CategoryLabel, SoundEntry[]>();
  const packs: Browse['packs'] = [];
  let readyCount = 0;
  for (const e of entries) {
    const a = availability(e);
    if (a.state === 'hidden' || !matches(e, query)) continue;
    const cat = categoryOf(e);
    if (category && cat !== category) continue;
    if (a.state === 'ready') {
      readyCount++;
      const list = groups.get(cat) ?? [];
      list.push(e);
      groups.set(cat, list);
    } else {
      packs.push({ entry: e, reason: a.reason });
    }
  }
  const order: CategoryLabel[] = [...CATEGORIES, OTHER_CATEGORY];
  return {
    groups: order.flatMap((c) => {
      const ready = groups.get(c);
      return ready ? [{ category: c, ready }] : [];
    }),
    packs,
    readyCount,
  };
}

/** One short line for the card; the long engineering licence text stays in the details. */
export function licenceLine(e: SoundEntry): string {
  return e.kind === 'builtin' ? 'Made by Composer AI DAW. No third party audio.' : e.licence;
}

/** Attribution wording to disclose, or null when none is required. */
export function attributionOf(e: SoundEntry): string | null {
  if (e.attribution_required !== 'yes') return null;
  return e.attribution_text ?? 'Credit the author and licence shown on the source page.';
}
