import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { BUILTIN_ENTRIES } from '../src/sounds/builtin';
import { CATEGORIES } from '../src/sounds/catalogue';
import { FALLBACK_ICON, instrumentIcon } from '../src/icons/instruments';

describe('instrument icons', () => {
  it('maps every sound category to its own icon', () => {
    for (const c of CATEGORIES) expect(instrumentIcon(c), c).not.toBe(FALLBACK_ICON);
  });

  it('maps every built in voice, from the catalogue file too', () => {
    for (const e of BUILTIN_ENTRIES) {
      expect(instrumentIcon(e.id), e.id).not.toBe(FALLBACK_ICON);
      expect(instrumentIcon(e), e.id).not.toBe(FALLBACK_ICON);
    }
    const yaml = readFileSync(new URL('../../../sounds/catalogue.yaml', import.meta.url), 'utf8');
    for (const m of yaml.matchAll(/^ {2}- id: (builtin-[a-z0-9-]+)/gm)) {
      expect(instrumentIcon(m[1]), m[1]).not.toBe(FALLBACK_ICON);
    }
  });

  it('maps the plain waveforms', () => {
    for (const w of ['sine', 'saw', 'square', 'triangle', 'noise'])
      expect(instrumentIcon(w), w).not.toBe(FALLBACK_ICON);
  });

  it('falls back safely for unknown input', () => {
    expect(instrumentIcon('mystery')).toBe(FALLBACK_ICON);
    expect(instrumentIcon(null)).toBe(FALLBACK_ICON);
    expect(instrumentIcon(undefined)).toBe(FALLBACK_ICON);
  });
});
