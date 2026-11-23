// D28: templates and recipes through both backends. The daemon side is an in-process stand in
// that records the wire params and answers from the same wasm exports.
import { describe, expect, it } from 'vitest';
import { DaemonBackend, WasmBlobCompiler, type RpcSocket } from '../src/backend/daemonBackend';
import type { Backend } from '../src/backend/types';
import { starterCommands } from '../src/fixtures';
import { WasmEditor } from '../src/wasm/editor';
import { counterIds, newBackend, wasmModule } from './helpers';

class RecipeDaemon implements RpcSocket {
  onmessage: ((data: string) => void) | null = null;
  onclose: (() => void) | null = null;
  readonly editor = WasmEditor.fromModule(wasmModule, 'daemon');
  readonly calls: { method: string; params: Record<string, unknown> }[] = [];

  send(data: string): void {
    const req = JSON.parse(data) as { id: number; method: string; params: Record<string, unknown> };
    this.calls.push({ method: req.method, params: req.params });
    queueMicrotask(() => {
      const { params } = req;
      const e = this.editor;
      // The daemon strips the changes flag before parsing; the stand in does the same.
      const { changes: _changes, ...rest } = params;
      void _changes;
      const table: Record<string, () => object> = {
        'templates.list': () => e.templatesListJson(),
        'recipes.list': () => e.recipesListJson(),
        'template.apply': () => e.templateApplyJson(rest),
        'recipe.plan': () => e.recipePlanJson(rest),
        'recipe.apply': () => e.recipeApplyJson(rest),
        'command.group': () => e.group(params.label as string, params.commands as never),
        'state.get': () => e.state(),
        'history.get': () => e.history(params as never),
        // The editor has no dry run, so a real undo then redo reports the depths.
        undo: () => {
          const u = e.undo();
          return u.result?.applied ? e.redo() : u;
        },
      };
      const raw = table[req.method]?.() ?? { error: { code: -32601, message: req.method, data: { code: 'method_not_found' } } };
      this.onmessage?.(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...raw }));
    });
  }

  close(): void {}
}

async function seeded(b: Backend): Promise<{ track: string; region: string }> {
  const r = await b.group('Starter', starterCommands(counterIds()));
  if (!r.ok) throw new Error(r.error.message);
  const st = await b.state();
  if (!st.ok) throw new Error('state');
  const track = st.value.project.tracks[0];
  const region = track ? Object.keys(track.regions)[0] : undefined;
  if (!track || !region) throw new Error('no clip');
  return { track: track.id, region };
}

function both(): { name: string; make: () => Backend }[] {
  return [
    { name: 'wasm', make: () => newBackend() },
    {
      name: 'daemon',
      make: () => new DaemonBackend(new RecipeDaemon(), new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob'))),
    },
  ];
}

describe.each(both())('templates and recipes ($name)', ({ make }) => {
  it('lists templates and recipes with typed parameters', async () => {
    const b = make();
    const t = await b.templatesList!();
    if (!t.ok) throw new Error(t.error.message);
    expect(t.value.length).toBeGreaterThan(0);
    expect(t.value[0]?.commandCount).toBeGreaterThan(0);
    const r = await b.recipesList!();
    if (!r.ok) throw new Error(r.error.message);
    expect(r.value.map((x) => x.name)).toContain('bass_from_chords');
    expect(r.value.find((x) => x.name === 'soften_velocity')?.params.map((p) => p.name)).toEqual(['amount', 'ceiling']);
    b.dispose();
  });

  it('plan changes nothing and apply is one undo step tagged recipe', async () => {
    const b = make();
    const scope = await seeded(b);
    const before = await b.state();
    if (!before.ok) throw new Error('state');
    const plan = await b.recipePlan!({ recipe: 'soften_velocity', params: { ceiling: 1 }, scope });
    if (!plan.ok) throw new Error(plan.error.message);
    expect(plan.value.commandCount).toBeGreaterThan(0);
    expect(plan.value.applies).toBe(true);
    const mid = await b.state();
    expect(mid.ok && mid.value.stateHash).toBe(before.value.stateHash);
    const applied = await b.recipeApply!({ recipe: 'soften_velocity', params: { ceiling: 1 }, scope });
    if (!applied.ok) throw new Error(applied.error.message);
    expect(applied.value.undoDepth).toBe(before.value.undoDepth + 1);
    expect(applied.value.stateHash).toBe(plan.value.stateHashAfter);
    const h = await b.history(0, 50);
    expect(h.ok && h.value.entries.at(-1)?.source).toBe('recipe');
    b.dispose();
  });

  it('reports an unknown recipe as a coded error', async () => {
    const b = make();
    const r = await b.recipePlan!({ recipe: 'nope' });
    expect(r.ok).toBe(false);
    expect(!r.ok && r.error.code).toBe('unknown_recipe');
    b.dispose();
  });

  it('applies a template as one step tagged template', async () => {
    const b = make();
    const t = await b.templatesList!();
    if (!t.ok || !t.value[0]) throw new Error('no template');
    const r = await b.templateApply!(t.value[0].id);
    if (!r.ok) throw new Error(r.error.message);
    expect(r.value.undoDepth).toBe(1);
    const h = await b.history(0, 5);
    expect(h.ok && h.value.entries[0]?.source).toBe('template');
    b.dispose();
  });
});
