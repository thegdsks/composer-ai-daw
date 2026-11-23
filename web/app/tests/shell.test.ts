import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { newId } from '../src/ids';
import { addNote } from '../src/state/commands';
import { findRegion, plainLabel, Store } from '../src/state/store';
import { counterIds, newBackend } from './helpers';

const src = join(dirname(fileURLToPath(import.meta.url)), '..', 'src');
const css = readFileSync(join(src, 'index.css'), 'utf8') + readFileSync(join(src, 'data-colors.css'), 'utf8');

function pair(token: string): [string, string] {
  const m = new RegExp(`--color-${token}:\\s*light-dark\\(([^,]+),\\s*([^)]+\\)?)\\);`).exec(css);
  if (!m?.[1] || !m[2]) throw new Error(`token ${token} missing`);
  return [m[1].trim(), m[2].trim()];
}

function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  const lin = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    const c = v / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * (lin[0] ?? 0) + 0.7152 * (lin[1] ?? 0) + 0.0722 * (lin[2] ?? 0);
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((hi ?? 0) + 0.05) / ((lo ?? 0) + 0.05);
}

describe('tokens v2', () => {
  it('defines the eight family hues, loop, ghost and record in both schemes', () => {
    for (const t of ['keys', 'strings', 'pads', 'bass', 'plucks', 'choir', 'bells', 'perc']) {
      const [light, dark] = pair(`family-${t}`);
      expect(light).toMatch(/^oklch\(0\.62 0\.08 \d+\)$/);
      expect(dark).toMatch(/^oklch\(0\.72 0\.08 \d+\)$/);
    }
    for (const t of ['loop', 'ghost', 'record', 'roll-outkey']) expect(pair(t)).toHaveLength(2);
  });

  it('keeps text readable and out of key rows close to the row shade, in both themes', () => {
    for (const i of [0, 1] as const) {
      expect(contrast(pair('ink')[i], pair('panel')[i])).toBeGreaterThanOrEqual(4.5);
      expect(contrast(pair('ink-dim')[i], pair('panel')[i])).toBeGreaterThanOrEqual(4.5);
      expect(contrast(pair('accent-ink')[i], pair('accent')[i])).toBeGreaterThanOrEqual(4.5);
      expect(contrast(pair('note')[i], pair('roll-bg')[i])).toBeGreaterThanOrEqual(3);
      const rowVsOut = contrast(pair('roll-row')[i], pair('roll-outkey')[i]);
      expect(rowVsOut).toBeGreaterThanOrEqual(1.05);
      expect(rowVsOut).toBeLessThanOrEqual(1.25);
    }
  });
});

describe('Store announcements', () => {
  it('turns command ids into plain words and leaves real labels alone', () => {
    expect(plainLabel('add_note')).toBe('Add note');
    expect(plainLabel('Move notes')).toBe('Move notes');
  });

  it('speaks the step label of a commit, then names the undone and redone step', async () => {
    const store = new Store(newBackend());
    await store.refresh();
    const said: string[] = [];
    store.onAnnounce((t) => said.push(t));
    await store.group('Starter', starterCommands(counterIds()));
    expect(said).toEqual(['Starter']);

    const snap = store.getSnapshot();
    const region = findRegion(snap.project, snap.selectedRegion)!.region;
    const r = await store.apply(addNote(newId(), region.id, 72, 480, 240));
    expect(r.ok).toBe(true);
    const label = said[1];
    expect(label).toBe('Add note');

    await store.undo();
    expect(said[2]).toBe(`Undid ${label}`);
    await store.redo();
    expect(said[3]).toBe(`Redid ${label}`);
  });

  it('stays quiet when a command changes nothing', async () => {
    const store = new Store(newBackend());
    await store.refresh();
    const said: string[] = [];
    store.onAnnounce((t) => said.push(t));
    await store.undo();
    expect(said).toEqual([]);
  });
});
