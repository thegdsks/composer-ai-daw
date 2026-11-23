// Palette rules: every described command is listed under a plain name, words match at word
// starts, disabled entries carry a reason, and forms build the right commands.
import { describe, expect, it } from 'vitest';
import type { CommandInfo } from '../src/model';
import {
  APP_ITEMS,
  buildCommand,
  buildItems,
  matchItems,
  planForm,
  type PaletteContext,
} from '../src/state/palette';
import { counterIds, newBackend } from './helpers';

async function load(): Promise<{ infos: CommandInfo[]; ctx: PaletteContext }> {
  const b = newBackend();
  const d = await b.describe();
  const s = await b.state();
  if (!d.ok || !s.ok) throw new Error('backend failed');
  b.dispose();
  return { infos: d.value, ctx: { project: s.value.project, track: null, region: null, notes: [] } };
}

describe('palette entries', () => {
  it('lists every described command with a plain name, a verb and search words', async () => {
    const { infos, ctx } = await load();
    const items = buildItems(infos, [], ctx);
    expect(items.length).toBe(infos.length + APP_ITEMS.length);
    for (const info of infos) {
      const item = items.find((i) => i.id === info.name);
      expect(item, info.name).toBeDefined();
      expect(info.verb, info.name).toBeTruthy();
      expect(info.keywords?.length, info.name).toBeGreaterThan(0);
      expect(item?.label).toBe(info.label);
      // A plain name is words, never a command id.
      expect(item?.label).not.toMatch(/_/);
    }
  });

  it('disables entries with a reason from commands.enabled or a missing selection', async () => {
    const b = newBackend();
    const { infos, ctx } = await load();
    const en = await b.enabled();
    if (!en.ok) throw new Error('enabled failed');
    const items = buildItems(infos, en.value, ctx);
    const rename = items.find((i) => i.id === 'rename_track');
    expect(rename?.enabled).toBe(false);
    expect(rename?.reason).toBeTruthy();
    const name = items.find((i) => i.id === 'set_project_name');
    expect(name?.enabled).toBe(true);
    const noNote = items.find((i) => i.id === 'transpose_notes');
    expect(noNote?.enabled).toBe(false);
    b.dispose();
  });

  it('matches at word starts, through keywords, and ranks by local use', async () => {
    const { infos, ctx } = await load();
    const items = buildItems(infos, [], ctx);
    expect(matchItems(items, 'add tr', {})[0]?.id).toBe('add_track');
    expect(matchItems(items, 'bpm', {}).map((i) => i.id)).toContain('set_tempo');
    expect(matchItems(items, 'ack', {}).length).toBe(0);
    const base = matchItems(items, '', {});
    const loved = base[base.length - 1];
    expect(loved).toBeDefined();
    if (loved) expect(matchItems(items, '', { [loved.id]: 5 })[0]?.id).toBe(loved.id);
  });
});

describe('palette forms', () => {
  it('turns beats and tempo into ticks and thousandths and fills ids from the selection', async () => {
    const { infos, ctx } = await load();
    const ids = counterIds();
    const tempo = infos.find((i) => i.name === 'set_tempo');
    if (!tempo) throw new Error('set_tempo missing');
    const plan = planForm(tempo, ctx, ids);
    expect(plan.ask.map((f) => f.name)).toEqual(['tick', 'mbpm']);
    const built = buildCommand(tempo, plan, { tick: '1', mbpm: '96' });
    expect(built).toEqual({ ok: true, command: { command: 'set_tempo', tick: 0, mbpm: 96000 } });

    const region = infos.find((i) => i.name === 'add_region');
    if (!region) throw new Error('add_region missing');
    const withTrack: PaletteContext = { ...ctx, track: 'aa' };
    const r = planForm(region, withTrack, ids);
    expect(r.auto.track).toBe('aa');
    expect(typeof r.auto.id).toBe('string');
    const bad = buildCommand(tempo, plan, { tick: '1', mbpm: 'fast' });
    expect(bad.ok).toBe(false);
  });
});
