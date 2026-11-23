import { describe, expect, it } from 'vitest';
import { config } from '../src/config';
import { isId } from '../src/ids';
import type { Project } from '../src/model';
import { loadAllTemplates, loadTemplate, parseTemplate, resolveTemplate, TEMPLATE_IDS } from '../src/templates';
import { BUILTIN_ENTRIES } from '../src/sounds/builtin';
import { counterIds, newBackend } from './helpers';

const FIRST_BAR = config.ticksPerQuarter * 4;
const LONG_DASHES = new RegExp(`[${String.fromCharCode(0x2013)}${String.fromCharCode(0x2014)}]`);

function notesOf(project: Project): { start: number; pitch: number }[] {
  const out: { start: number; pitch: number }[] = [];
  for (const track of project.tracks) {
    for (const region of Object.values(track.regions)) {
      if (!('midi' in region.content)) continue;
      for (const n of Object.values(region.content.midi.notes)) out.push({ start: region.start + n.start, pitch: n.pitch });
    }
  }
  return out;
}

// The blank template is empty on purpose, so it is the only one without sound.
const SILENT: readonly string[] = ['blank'];

describe('starter templates', () => {
  const all = loadAllTemplates();

  for (const id of TEMPLATE_IDS) {
    it(`${id}: replays in one group with no errors`, async () => {
      const template = all[id];
      const backend = newBackend(id);
      const { commands } = resolveTemplate(template, counterIds());
      const reply = await backend.group(template.name, commands);
      expect(reply, JSON.stringify(reply)).toMatchObject({ ok: true });
      const state = await backend.state();
      if (!state.ok) throw new Error('state failed');
      expect(state.value.undoDepth).toBe(1);
      expect(state.value.project.tracks.length).toBeGreaterThan(0);
      expect(Object.values(state.value.project.tempo_map.tempos)).toEqual([template.tempo * 1000]);
      const notes = notesOf(state.value.project);
      if (SILENT.includes(id)) {
        expect(notes).toHaveLength(0);
      } else {
        expect(notes.length).toBeGreaterThan(0);
        // sound within the first bar
        expect(Math.min(...notes.map((n) => n.start))).toBeLessThan(FIRST_BAR);
      }
      backend.dispose();
    });
  }

  it('describes every template in a sentence without long dashes', () => {
    for (const id of TEMPLATE_IDS) {
      const t = all[id];
      expect(t.description.length).toBeGreaterThan(20);
      expect(JSON.stringify(t)).not.toMatch(LONG_DASHES);
    }
  });

  it('gives a fresh set of ids on every resolve and reuses one id per placeholder', () => {
    const t = loadTemplate('cinematic');
    const a = resolveTemplate(t);
    const b = resolveTemplate(t);
    const idsA = new Set(a.ids.values());
    expect(idsA.size).toBe(a.ids.size);
    const idsB = new Set(b.ids.values());
    for (const id of idsA) {
      expect(isId(id)).toBe(true);
      expect(idsB.has(id)).toBe(false);
    }
    expect(JSON.stringify(a.commands)).not.toContain('$');
  });

  it('resolves first clip and loop range for the client', () => {
    const r = resolveTemplate(loadTemplate('calm-hour'), counterIds());
    expect(r.ids.get('$r1')).toBeDefined();
    expect(r.template.loop).toEqual({ start: 0, end: 8 * FIRST_BAR });
  });

  it('gives every track a built in voice that the catalogue lists', async () => {
    const known = new Set(BUILTIN_ENTRIES.map((e) => e.id));
    for (const id of TEMPLATE_IDS) {
      const template = all[id];
      const backend = newBackend(`sound-${id}`);
      await backend.group(template.name, resolveTemplate(template, counterIds()).commands);
      const state = await backend.state();
      if (!state.ok) throw new Error('state failed');
      for (const track of state.value.project.tracks) {
        expect(track.sound, `${id}: ${track.name}`).toBeDefined();
        expect(known.has(track.sound ?? '')).toBe(true);
      }
      backend.dispose();
    }
  });

  it('rejects malformed template data', () => {
    expect(() => parseTemplate(null)).toThrow();
    expect(() => parseTemplate({ ...all.blank, tempo: 0 })).toThrow(/tempo/);
    expect(() => parseTemplate({ ...all.blank, ticks_per_quarter: 480 })).toThrow(/ticks/);
    expect(() => parseTemplate({ ...all.blank, commands: [1] })).toThrow(/commands/);
  });
});
