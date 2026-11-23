import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SoundEntry } from '../src/backend/types';
import { config } from '../src/config';
import { matchShortcut, SHORTCUTS } from '../src/input/keymap';
import { browse } from '../src/sounds/catalogue';
import { flatten, offsets, scrollToShow, stepIndex, visibleRange } from '../src/ui/libraryList';
import { clampWidth, getLibrary, parseLibrary, reloadLibrary, setLibrary, toggleLibrary } from '../src/ui/libraryState';

function memoryStorage(): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    clear: () => data.clear(),
    getItem: (k) => data.get(k) ?? null,
    key: (i) => [...data.keys()][i] ?? null,
    removeItem: (k) => void data.delete(k),
    setItem: (k, v) => void data.set(k, v),
  };
}

function entry(id: string, name: string, kind = 'builtin'): SoundEntry {
  return { id, name, kind, licence: 'CC0', instruments: `${name} voice`, bundle: kind === 'builtin' ? 'yes' : 'maybe' };
}

describe('library state', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', memoryStorage());
    reloadLibrary();
  });
  afterEach(() => vi.unstubAllGlobals());

  it('is open at its default width on first run', () => {
    expect(parseLibrary(null)).toEqual({ open: true, width: config.libraryWidthDefault });
    expect(getLibrary().open).toBe(true);
  });

  it('falls back to the first run state when the stored value is damaged', () => {
    expect(parseLibrary('{nope').open).toBe(true);
    expect(parseLibrary('{"open":"yes","width":"wide"}')).toEqual({ open: true, width: config.libraryWidthDefault });
  });

  it('clamps the width to the configured limits', () => {
    expect(clampWidth(1)).toBe(config.libraryWidthMin);
    expect(clampWidth(99999)).toBe(config.libraryWidthMax);
    expect(clampWidth(Number.NaN)).toBe(config.libraryWidthDefault);
    expect(parseLibrary('{"open":true,"width":5}').width).toBe(config.libraryWidthMin);
    setLibrary({ width: 99999 });
    expect(getLibrary().width).toBe(config.libraryWidthMax);
  });

  it('collapses to a rail, reopens, and remembers both', () => {
    toggleLibrary();
    expect(getLibrary().open).toBe(false);
    reloadLibrary();
    expect(getLibrary().open).toBe(false);
    toggleLibrary();
    setLibrary({ width: 333 });
    reloadLibrary();
    expect(getLibrary()).toEqual({ open: true, width: 333 });
  });

  it('does not save while a drag is in progress', () => {
    setLibrary({ width: 300 }, false);
    reloadLibrary();
    expect(getLibrary().width).toBe(config.libraryWidthDefault);
  });
});

function key(init: Partial<KeyboardEvent>): KeyboardEvent {
  return { ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...init } as KeyboardEvent;
}

describe('library shortcut', () => {
  it('Shift+B is the one library key and is listed in the keymap', () => {
    expect(matchShortcut(key({ key: 'B', code: 'KeyB', shiftKey: true }))).toBe('library');
    expect(matchShortcut(key({ key: 'b', code: 'KeyB' }))).toBeNull();
    expect(SHORTCUTS.some((s) => s.id === 'library')).toBe(true);
  });
});

describe('library list', () => {
  const entries = [
    entry('builtin-soft-piano', 'Soft Piano'),
    entry('builtin-warm-pad', 'Warm Pad'),
    entry('builtin-bell', 'Bell'),
    entry('pack-sf2', 'Piano Pack', 'soundfont'),
  ];

  it('groups by category and keeps packs last, shared with the Sound tab rules', () => {
    const items = flatten(browse(entries, '', null), new Set());
    expect(items.map((i) => i.kind)).toEqual(['group', 'sound', 'group', 'sound', 'group', 'sound', 'pack']);
  });

  it('search narrows the list and collapsing hides a group sounds', () => {
    expect(flatten(browse(entries, 'warm', null), new Set()).filter((i) => i.kind === 'sound')).toHaveLength(1);
    const items = flatten(browse(entries, '', null), new Set(['Keys']));
    expect(items.filter((i) => i.kind === 'sound')).toHaveLength(2);
    expect(items[0]).toMatchObject({ kind: 'group', collapsed: true });
  });

  it('windows long lists and keeps the active row in view', () => {
    const many = Array.from({ length: 5000 }, (_, i) => entry(`builtin-s${i}`, `Sound ${i}`));
    const items = flatten(browse(many, '', null), new Set());
    const tops = offsets(items);
    const { first, last } = visibleRange(tops, 100000, 400);
    expect(last - first).toBeLessThan(40);
    expect(tops[first] ?? 0).toBeLessThanOrEqual(100000);
    expect(scrollToShow(tops, 10, 0, 100)).toBeGreaterThan(0);
    expect(scrollToShow(tops, 0, 0, 100)).toBe(0);
  });

  it('steps the active index and stays inside the list', () => {
    expect(stepIndex(-1, 'down', 3)).toBe(0);
    expect(stepIndex(2, 'down', 3)).toBe(2);
    expect(stepIndex(0, 'up', 3)).toBe(0);
    expect(stepIndex(1, 'end', 3)).toBe(2);
    expect(stepIndex(0, 'down', 0)).toBe(-1);
  });
});
