// One contract suite, run against both backends on the real wasm module. The daemon runs
// against an in-process stand in that speaks composerd's JSON-RPC method names.
import { afterEach, describe, expect, it } from 'vitest';
import { DaemonBackend, WasmBlobCompiler, tokenFromHash, type RpcSocket } from '../src/backend/daemonBackend';
import type { Backend } from '../src/backend/types';
import { renderBlob } from '../src/audio/offline';
import { parseWav } from '../src/audio/wav';
import { starterCommands, stressCommands } from '../src/fixtures';
import type { Command } from '../src/model';
import { WasmEditor } from '../src/wasm/editor';
import { readBlobInfo } from '../src/wasm/blob';
import { counterIds, newBackend, wasmModule } from './helpers';

/** Minimal composerd: same method names and result shapes, backed by a wasm editor. */
class FakeDaemon implements RpcSocket {
  onmessage: ((data: string) => void) | null = null;
  onclose: (() => void) | null = null;
  readonly editor = WasmEditor.fromModule(wasmModule, 'daemon');
  closed = false;
  /** Files dir stand in: name to project JSON. */
  readonly files = new Map<string, unknown>();
  readonly calls: { method: string; params: Record<string, unknown> }[] = [];

  send(data: string): void {
    const req = JSON.parse(data) as { id: number; method: string; params: Record<string, unknown> };
    queueMicrotask(() => this.handle(req));
  }

  close(): void {
    this.closed = true;
  }

  private reply(id: number, raw: { result?: unknown; error?: { code: number; message: string; data: unknown } }): void {
    this.onmessage?.(JSON.stringify({ jsonrpc: '2.0', id, ...raw }));
  }

  private handle(req: { id: number; method: string; params: Record<string, unknown> }): void {
    const { id, method, params } = req;
    this.calls.push({ method, params });
    const e = this.editor;
    const appError = (code: string, message: string): void =>
      this.reply(id, { error: { code: -32000, message, data: { code } } });
    const mutating = (raw: ReturnType<WasmEditor['apply']>): void => {
      this.reply(id, raw);
      if (raw.result) {
        this.onmessage?.(
          JSON.stringify({ jsonrpc: '2.0', method: 'state.changed', params: { hash: e.stateHash, cause: method } }),
        );
      }
    };
    switch (method) {
      case 'subscribe':
        return this.reply(id, { result: { subscribed: true, state_hash: e.stateHash } });
      case 'command.apply':
        return mutating(e.apply(params.command as Command));
      case 'command.group':
        return mutating(e.group(params.label as string, params.commands as Command[]));
      case 'undo':
        if (params.dry_run) {
          // The editor has no dry run, so report depths from a real undo then redo.
          const u = e.undo();
          if (u.result?.applied) {
            const r = e.redo();
            return this.reply(id, { result: { ...(r.result ?? {}), dry_run: true } });
          }
          return this.reply(id, u);
        }
        if (typeof params.steps === 'number') return mutating(e.jump(true, params.steps));
        return mutating(e.undo());
      case 'redo':
        if (typeof params.steps === 'number') return mutating(e.jump(false, params.steps));
        return mutating(e.redo());
      case 'history.get':
        return this.reply(id, e.history(params as never));
      case 'group.begin':
        return this.reply(id, e.groupBegin(params.label as string, params.source as string | undefined));
      case 'group.commit':
        return mutating(e.groupClose(params.token as string, true));
      case 'group.rollback':
        return mutating(e.groupClose(params.token as string, false));
      case 'commands.enabled':
        return this.reply(id, e.enabled((params.context ?? {}) as never));
      case 'project.save': {
        const path = params.path as string;
        if (this.files.has(path) && !params.overwrite) return appError('file_exists', 'exists');
        this.files.set(path, e.state().result?.project);
        return this.reply(id, { result: { path, bytes: 128, state_hash: e.stateHash } });
      }
      case 'project.load': {
        const path = params.path as string;
        if (!this.files.has(path)) return appError('file_not_found', 'missing');
        const project = this.files.get(path) as { name: string; tracks: unknown[] };
        const raw = e.load(project as never);
        if (raw.error) return appError('malformed', raw.error.message);
        return mutating({ result: { ...raw.result, path, name: project.name, tracks: project.tracks.length } } as never);
      }
      case 'transport.get':
        return this.reply(id, e.transportGet());
      case 'transport.set':
        return this.reply(id, e.transportSet(params as never));
      case 'sounds.list':
        return this.reply(id, {
          result: {
            count: 1,
            entries: [
              {
                id: 'freepats-upright-piano-kw',
                name: 'FreePats Upright Piano KW',
                kind: 'sfz',
                licence: 'CC0 1.0',
                instruments: 'piano',
                bundle: 'yes',
              },
            ],
            gaps: ['no drums'],
          },
        });
      case 'state.get':
        return this.reply(id, e.state());
      case 'commands.describe':
        return this.reply(id, e.describe());
      default:
        return this.reply(id, { error: { code: -32601, message: method, data: { code: 'method_not_found' } } });
    }
  }
}

const makers: [string, () => Promise<Backend>][] = [
  ['WasmBackend', async () => newBackend()],
  [
    'DaemonBackend',
    async () => new DaemonBackend(new FakeDaemon(), new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob'))),
  ],
];

describe.each(makers)('%s contract', (_name, make) => {
  let backend: Backend | null = null;
  afterEach(() => {
    backend?.dispose();
    backend = null;
  });
  const fresh = async (): Promise<Backend> => {
    backend = await make();
    return backend;
  };

  it('applies a command and reports the summary', async () => {
    const b = await fresh();
    const r = await b.apply({ command: 'add_track', id: '1'.padStart(32, '0'), name: 'Lead', kind: 'instrument', index: null });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value.changed).toBe(true);
      expect(r.value.undoDepth).toBe(1);
      expect(r.value.stateHash).toMatch(/^[0-9a-f]{16}$/);
    }
    const s = await b.state();
    expect(s.ok && s.value.project.tracks.map((t) => t.name)).toEqual(['Lead']);
    expect(s.ok && s.value.stateHash).toBe(r.ok ? r.value.stateHash : '');
  });

  it('round trips undo and redo to identical hashes', async () => {
    const b = await fresh();
    const ids = counterIds();
    const s0 = await b.state();
    const h0 = s0.ok ? s0.value.stateHash : '';
    const g = await b.group('starter', starterCommands(ids));
    expect(g.ok).toBe(true);
    const h1 = g.ok ? g.value.stateHash : '';
    expect(h1).not.toBe(h0);
    const u = await b.undo();
    expect(u.ok && u.value.applied).toBe(true);
    expect(u.ok && u.value.stateHash).toBe(h0);
    const r = await b.redo();
    expect(r.ok && r.value.applied).toBe(true);
    expect(r.ok && r.value.stateHash).toBe(h1);
    const s = await b.state();
    expect(s.ok && s.value.undoDepth).toBe(1);
    expect(s.ok && s.value.redoDepth).toBe(0);
  });

  it('returns stable error codes and leaves state alone', async () => {
    const b = await fresh();
    const before = await b.state();
    const r = await b.apply({ command: 'delete_track', track: 'ff'.repeat(16) });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.code).toBe('not_found');
    const after = await b.state();
    expect(after.ok && before.ok && after.value.stateHash).toBe(before.ok ? before.value.stateHash : '');
  });

  it('rolls a failing group back and names the failing index', async () => {
    const b = await fresh();
    const ids = counterIds();
    const track = ids();
    const r = await b.group('bad', [
      { command: 'add_track', id: track, name: 'A', kind: 'instrument', index: null },
      { command: 'delete_track', track: 'ee'.repeat(16) },
    ]);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.index).toBe(1);
    const s = await b.state();
    expect(s.ok && s.value.project.tracks).toHaveLength(0);
  });

  it('round trips transport fields without touching the state hash', async () => {
    const b = await fresh();
    const before = await b.state();
    const h0 = before.ok ? before.value.stateHash : '';
    const first = await b.transportGet();
    expect(first.ok && first.value.loop).toEqual({ on: false, start: 0, end: 0 });
    expect(first.ok && first.value.follow).toBe(true);
    const set = await b.transportSet({
      tick: 960,
      loop: { on: true, start: 0, end: 7680 },
      metronome: { on: true },
      countInBars: 2,
      follow: false,
    });
    expect(set.ok).toBe(true);
    const got = await b.transportGet();
    expect(got.ok && got.value).toMatchObject({
      playheadTick: 960,
      loop: { on: true, start: 0, end: 7680 },
      metronome: { on: true },
      countInBars: 2,
      follow: false,
      playing: false,
    });
    const dry = await b.transportSet({ loop: { end: 3840 } }, true);
    expect(dry.ok && dry.value.loop.end).toBe(3840);
    const after = await b.transportGet();
    expect(after.ok && after.value.loop.end).toBe(7680);
    for (const bad of [{}, { loop: { start: 9000 } }, { countInBars: 99 }, { tick: -1 }]) {
      const r = await b.transportSet(bad);
      expect(r.ok).toBe(false);
      if (!r.ok) expect(r.error.code).toBe('invalid');
    }
    const s = await b.state();
    expect(s.ok && s.value.stateHash).toBe(h0);
    expect(s.ok && s.value.undoDepth).toBe(0);
  });

  it('reads history, jumps by steps and returns to the same hashes', async () => {
    const b = await fresh();
    const hashes: string[] = [];
    for (const name of ['one', 'two', 'three']) {
      const r = await b.apply({ command: 'set_project_name', name });
      if (r.ok) hashes.push(r.value.stateHash);
    }
    const page = await b.history(0);
    expect(page.ok && page.value.entries.map((e) => e.source)).toEqual(['user', 'user', 'user']);
    expect(page.ok && [page.value.cursor, page.value.total]).toEqual([3, 3]);
    const back = await b.jump('undo', 2);
    expect(back.ok && back.value.stateHash).toBe(hashes[0]);
    expect(back.ok && back.value.stepsApplied).toBe(2);
    const fwd = await b.jump('redo', 1);
    expect(fwd.ok && fwd.value.stateHash).toBe(hashes[1]);
    const after = await b.history(0, 2);
    expect(after.ok && [after.value.entries.length, after.value.cursor, after.value.total]).toEqual([2, 2, 3]);
  });

  it('rolls an open group back to the exact hash and commits another as one step', async () => {
    const b = await fresh();
    const before = await b.state();
    const open = await b.groupBegin('Try a name', 'user');
    expect(open.ok).toBe(true);
    if (!open.ok || !before.ok) return;
    await b.apply({ command: 'set_project_name', name: 'trial' });
    const rolled = await b.groupRollback(open.value.token);
    expect(rolled.ok && rolled.value.stateHash).toBe(before.value.stateHash);
    const second = await b.groupBegin('Keep a name', 'template');
    if (!second.ok) throw new Error('group.begin failed');
    await b.apply({ command: 'set_project_name', name: 'a' });
    await b.apply({ command: 'set_project_name', name: 'b' });
    const kept = await b.groupCommit(second.value.token);
    expect(kept.ok && kept.value.undoDepth).toBe(1);
    const page = await b.history(0);
    expect(page.ok && [page.value.entries.length, page.value.entries[0]?.source, page.value.entries[0]?.commandCount]).toEqual([1, 'template', 2]);
  });

  it('says why a command is not available', async () => {
    const b = await fresh();
    const r = await b.enabled();
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const move = r.value.find((c) => c.name === 'move_track');
    expect(move?.enabled).toBe(false);
    expect(move?.reason).toBeTruthy();
  });

  it('describes the commands', async () => {
    const b = await fresh();
    const d = await b.describe();
    expect(d.ok && d.value.map((c) => c.name)).toContain('add_note');
  });

  it('compiles an engine blob that renders sound', async () => {
    const b = await fresh();
    await b.group('starter', starterCommands(counterIds()));
    const r = await b.engineBlob(48000);
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    const info = readBlobInfo(r.value);
    expect(info.trackCount).toBe(2);
    expect(info.eventCount).toBeGreaterThan(0);
    const out = await renderBlob(wasmModule, r.value);
    expect(out.peak).toBeGreaterThan(0.01);
    expect(out.clippedFrames).toBe(0);
    const wav = parseWav(out.wav);
    expect(wav).toMatchObject({ channels: 2, sampleRate: 48000, bitsPerSample: 16 });
    expect(wav.frames).toBe(out.frames);
  });

  it('builds a large project from group commands', async () => {
    const b = await fresh();
    const fx = stressCommands(3000, counterIds(), 1000);
    expect((await b.group('setup', fx.setup)).ok).toBe(true);
    for (const g of fx.groups) expect((await b.group('notes', g)).ok).toBe(true);
    const s = await b.state();
    if (!s.ok) throw new Error('state failed');
    const region = Object.values(s.value.project.tracks[0]?.regions ?? {})[0];
    expect(region && 'midi' in region.content ? Object.keys(region.content.midi.notes) : []).toHaveLength(3000);
  });
});

describe('WasmBackend load', () => {
  it('replaces the project and clears history', async () => {
    const a = newBackend('a');
    await a.group('starter', starterCommands(counterIds()));
    const s = await a.state();
    if (!s.ok) throw new Error('state failed');
    const b = newBackend('b');
    const l = await b.load(s.value.project);
    expect(l.ok && l.value.undoDepth).toBe(0);
    const t = await b.state();
    expect(t.ok && t.value.stateHash).toBe(s.value.stateHash);
    const bad = await b.load({ ...s.value.project, tracks: [{ ...s.value.project.tracks[0]!, kind: 'nope' as never }] });
    expect(bad.ok).toBe(false);
    a.dispose();
    b.dispose();
  });
});

describe('DaemonBackend files and sounds', () => {
  const make = (): { fake: FakeDaemon; b: DaemonBackend } => {
    const fake = new FakeDaemon();
    return { fake, b: new DaemonBackend(fake, new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob'))) };
  };

  it('saves and opens a project by relative name', async () => {
    const { fake, b } = make();
    await b.group('starter', starterCommands(counterIds()));
    const saved = await b.saveProject('song.cmpf', false);
    expect(saved.ok && saved.value.path).toBe('song.cmpf');
    expect(fake.calls.at(-1)).toEqual({ method: 'project.save', params: { path: 'song.cmpf', overwrite: false } });
    await b.apply({ command: 'set_project_name', name: 'Changed' });
    const opened = await b.openProject('song.cmpf');
    expect(opened.ok).toBe(true);
    if (opened.ok) expect(opened.value.tracks).toBe(2);
    const s = await b.state();
    expect(s.ok && s.value.project.tracks).toHaveLength(2);
    expect(b.hasFiles).toBe(true);
    b.dispose();
  });

  it('passes the daemon error codes through', async () => {
    const { b } = make();
    const missing = await b.openProject('nope.cmpf');
    expect(!missing.ok && missing.error.code).toBe('file_not_found');
    await b.saveProject('a.cmpf', false);
    const again = await b.saveProject('a.cmpf', false);
    expect(!again.ok && again.error.code).toBe('file_exists');
    const forced = await b.saveProject('a.cmpf', true);
    expect(forced.ok).toBe(true);
    b.dispose();
  });

  it('lists the sound catalogue', async () => {
    const { b } = make();
    const r = await b.sounds();
    expect(r.ok && r.value.entries.map((e) => e.name)).toEqual(['FreePats Upright Piano KW']);
    expect(r.ok && r.value.gaps).toEqual(['no drums']);
    b.dispose();
  });

  it('the in-browser backend has no files and lists only the built in voices', async () => {
    const w = newBackend();
    expect(w.hasFiles).toBe(false);
    const s = await w.sounds();
    expect(s.ok && s.value.entries.map((e) => e.kind)).toEqual(Array(16).fill('builtin'));
    const o = await w.openProject();
    expect(!o.ok && o.error.code).toBe('unsupported');
    w.dispose();
  });
});

describe('DaemonBackend specifics', () => {
  it('tells subscribers about changes pushed by the daemon', async () => {
    const fake = new FakeDaemon();
    const b = new DaemonBackend(fake, new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob')));
    const seen: string[] = [];
    b.subscribe((c) => seen.push(c.cause));
    await b.apply({ command: 'set_project_name', name: 'Renamed' });
    expect(seen).toEqual(['command.apply']);
    b.dispose();
    expect(fake.closed).toBe(true);
  });

  it('reuses the compiled blob until the state changes', async () => {
    const b = new DaemonBackend(new FakeDaemon(), new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob')));
    await b.group('starter', starterCommands(counterIds()));
    const first = await b.engineBlob(48000);
    const second = await b.engineBlob(48000);
    expect(first.ok && second.ok && first.value === second.value).toBe(true);
    await b.apply({ command: 'set_tempo', tick: 0, mbpm: 90000 });
    const third = await b.engineBlob(48000);
    expect(third.ok && first.ok && third.value === first.value).toBe(false);
    b.dispose();
  });

  it('refuses to load a project and says why', async () => {
    const b = new DaemonBackend(new FakeDaemon(), new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob')));
    const r = await b.load();
    expect(!r.ok && r.error.code).toBe('unsupported');
    b.dispose();
  });

  it('fails calls once the connection closes', async () => {
    const fake = new FakeDaemon();
    const b = new DaemonBackend(fake, new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob')));
    fake.onclose?.();
    const r = await b.apply({ command: 'set_project_name', name: 'x' });
    expect(!r.ok && r.error.code).toBe('connection_closed');
  });

  it('reads the pairing token from the url hash', () => {
    expect(tokenFromHash('#token=abc123')).toBe('abc123');
    expect(tokenFromHash('#a=1&token=zz')).toBe('zz');
    expect(tokenFromHash('')).toBeNull();
  });
});
