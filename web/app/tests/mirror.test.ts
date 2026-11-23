// Change feed mirror (D25): random sessions on the real wasm editor, a two client daemon
// stand in, and the hash guard that falls back to a full fetch.
import { describe, expect, it } from 'vitest';
import { DaemonBackend, WasmBlobCompiler, type RpcSocket } from '../src/backend/daemonBackend';
import { ProjectMirror } from '../src/backend/mirror';
import type { Reply, Summary } from '../src/backend/types';
import type { Command, Project } from '../src/model';
import { WasmEditor } from '../src/wasm/editor';
import { counterIds, newBackend, wasmModule } from './helpers';
import { randomCommand, rng } from './randomSession';

describe('ProjectMirror over the wasm editor', () => {
  it('equals the backend state hash and project after 1000 random commands and undos', async () => {
    const backend = newBackend('random');
    // Record which change kinds the session produced, so the run cannot pass by doing little.
    const ops = new Set<string>();
    for (const name of ['apply', 'group', 'undo', 'redo'] as const) {
      const original = backend[name].bind(backend) as (...a: never[]) => Promise<Reply<Summary>>;
      (backend as unknown as Record<string, unknown>)[name] = async (...a: never[]) => {
        const r = await original(...a);
        if (r.ok) for (const c of r.value.feed?.changes ?? []) ops.add(c.op);
        return r;
      };
    }
    const mirror = new ProjectMirror(backend);
    await mirror.open();
    const rand = rng(20251123);
    const id = counterIds();
    let applied = 0;
    for (let i = 0; i < 1000; i++) {
      const step = randomCommand(mirror.project as Project, rand, id);
      if (step === 'undo') await mirror.undo();
      else if (step === 'redo') await mirror.redo();
      else if (Array.isArray(step)) await mirror.group('pair', step);
      else await mirror.apply(step);
      const truth = await backend.state();
      expect(truth.ok).toBe(true);
      if (!truth.ok) return;
      expect(mirror.hash, `step ${i}`).toBe(truth.value.stateHash);
      if (i % 25 === 0) expect(await mirror.verify(), `deep compare at step ${i}`).toBe(true);
      applied = mirror.stats.feedsApplied;
    }
    expect(await mirror.verify()).toBe(true);
    // The feed did the work: one open fetch and no fallbacks.
    expect(mirror.stats.fullFetches).toBe(1);
    expect(mirror.stats.fallbacks).toBe(0);
    expect(applied).toBeGreaterThan(400);
    for (const op of ['new_track', 'delete_track', 'new_region', 'delete_region', 'new_note', 'delete_note', 'set', 'set_tempo', 'set_meter', 'set_key']) {
      expect(ops.has(op), `no ${op} change in the session`).toBe(true);
    }
    const p = mirror.project as Project;
    expect(p.tracks.length).toBeGreaterThan(0);
    backend.dispose();
  });

  it('shares unchanged tracks with the previous project object', async () => {
    const backend = newBackend('share');
    const seen: Project[] = [];
    const mirror = new ProjectMirror(backend, { onChange: (p) => seen.push(p) });
    await mirror.open();
    const id = counterIds();
    const [a, b] = [id(), id()];
    await mirror.apply({ command: 'add_track', id: a, name: 'A', kind: 'instrument', index: null });
    await mirror.apply({ command: 'add_track', id: b, name: 'B', kind: 'instrument', index: null });
    const before = mirror.project as Project;
    await mirror.apply({ command: 'rename_track', track: b, name: 'B2' });
    const after = mirror.project as Project;
    expect(after).not.toBe(before);
    expect(after.tracks[0]).toBe(before.tracks[0]);
    expect(after.tracks[1]).not.toBe(before.tracks[1]);
    expect(before.tracks[1]?.name).toBe('B');
    expect(seen.length).toBeGreaterThan(2);
    backend.dispose();
  });

  it('never applies a feed onto a base it does not match and fetches the full state instead', async () => {
    const backend = newBackend('guard');
    const mirror = new ProjectMirror(backend);
    await mirror.open();
    const id = counterIds();
    await mirror.apply({ command: 'add_track', id: id(), name: 'A', kind: 'instrument', index: null });
    // An edit the mirror never saw: its next feed starts from a hash the mirror does not hold.
    await backend.apply({ command: 'add_track', id: id(), name: 'Hidden', kind: 'instrument', index: null });
    const r = await mirror.apply({ command: 'add_track', id: id(), name: 'C', kind: 'instrument', index: null });
    expect(r.ok).toBe(true);
    expect(mirror.stats.fallbacks).toBe(1);
    expect(mirror.project?.tracks.map((t) => t.name)).toEqual(['A', 'Hidden', 'C']);
    expect(await mirror.verify()).toBe(true);
    backend.dispose();
  });

  it('falls back to a full fetch when a feed does not fit the mirror', async () => {
    const backend = newBackend('bad');
    const mirror = new ProjectMirror(backend);
    await mirror.open();
    const id = counterIds();
    const a = id();
    await mirror.apply({ command: 'add_track', id: a, name: 'A', kind: 'instrument', index: null });
    // Corrupt the mirror's copy, keeping its hash: the next feed renames from a value it lacks.
    const p = mirror.project as Project;
    const t = p.tracks[0];
    if (t) t.name = 'tampered';
    await mirror.apply({ command: 'rename_track', track: a, name: 'B' });
    expect(mirror.stats.fallbacks).toBe(1);
    expect(mirror.project?.tracks[0]?.name).toBe('B');
    expect(await mirror.verify()).toBe(true);
    backend.dispose();
  });

  it('reports a feed too large to list as full and still ends on the right state', async () => {
    const backend = newBackend('big');
    const mirror = new ProjectMirror(backend);
    await mirror.open();
    const id = counterIds();
    const track = id();
    const region = id();
    const notes: Command[] = [];
    for (let i = 0; i < 6000; i++) {
      notes.push({ command: 'add_note', id: id(), region, pitch: 40 + (i % 40), velocity: 90, start: (i % 400) * 40, duration: 20 });
    }
    await mirror.group('bulk', [
      { command: 'add_track', id: track, name: 'T', kind: 'instrument', index: null },
      { command: 'add_region', id: region, track, start: 0, length: 960 * 40, kind: 'midi' },
      ...notes,
    ]);
    expect(mirror.stats.fallbacks).toBe(1);
    expect(await mirror.verify()).toBe(true);
    backend.dispose();
  });
});

/** Daemon stand in with two sockets: same method names, pushes with feeds to every client. */
class FakeHub {
  readonly editor = WasmEditor.fromModule(wasmModule, 'hub');
  readonly sockets: HubSocket[] = [];
  readonly log: { base: string; hash: string; list: unknown[] | null }[] = [];

  constructor() {
    this.editor.setFeed(5000);
  }

  connect(): HubSocket {
    const s = new HubSocket(this);
    this.sockets.push(s);
    return s;
  }

  handle(from: HubSocket, req: { id: number; method: string; params: Record<string, unknown> }): void {
    const e = this.editor;
    const { id, method, params } = req;
    let raw: { result?: unknown; error?: unknown };
    if (method === 'command.apply') raw = e.apply(params.command as Command);
    else if (method === 'command.group') raw = e.group(params.label as string, params.commands as Command[]);
    else if (method === 'undo') raw = params.dry_run ? { result: { undo_depth: 0, redo_depth: 0 } } : e.undo();
    else if (method === 'redo') raw = e.redo();
    else if (method === 'state.get') raw = e.state();
    else if (method === 'subscribe') raw = { result: { subscribed: true } };
    else if (method === 'changes.since') raw = { result: this.since(params.hash as string) };
    else raw = { error: { code: -32601, message: method, data: { code: 'method_not_found' } } };
    from.deliver({ jsonrpc: '2.0', id, ...raw });
    const feed = (raw.result as { changes?: { base_hash: string; hash: string; list?: unknown[] } } | undefined)?.changes;
    if (feed && feed.base_hash !== feed.hash) {
      this.log.push({ base: feed.base_hash, hash: feed.hash, list: feed.list ?? null });
      for (const s of this.sockets) {
        s.deliver({ jsonrpc: '2.0', method: 'state.changed', params: { hash: feed.hash, cause: method, changes: feed } });
      }
    }
  }

  private since(hash: string): { base_hash: string; hash: string; list?: unknown[]; full?: boolean } {
    const head = (this.editor.state().result as { state_hash: string }).state_hash;
    if (hash === head) return { base_hash: hash, hash: head, list: [] };
    const start = this.log.map((f) => f.base).lastIndexOf(hash);
    if (start < 0) return { base_hash: hash, hash: head, full: true };
    const list: unknown[] = [];
    for (const f of this.log.slice(start)) {
      if (!f.list) return { base_hash: hash, hash: head, full: true };
      list.push(...f.list);
    }
    return { base_hash: hash, hash: head, list };
  }
}

class HubSocket implements RpcSocket {
  onmessage: ((data: string) => void) | null = null;
  onclose: (() => void) | null = null;
  /** When set, pushes are held back so a client can fall behind. */
  muted = false;
  private held: string[] = [];

  constructor(private readonly hub: FakeHub) {}

  send(data: string): void {
    const req = JSON.parse(data) as { id: number; method: string; params: Record<string, unknown> };
    queueMicrotask(() => this.hub.handle(this, req));
  }

  close(): void {}

  deliver(msg: object): void {
    const text = JSON.stringify(msg);
    const isPush = 'method' in msg;
    if (isPush && this.muted) this.held.push(text);
    else this.onmessage?.(text);
  }

  dropHeld(n: number): void {
    this.held = this.held.slice(n);
  }

  flush(): void {
    const all = this.held;
    this.held = [];
    for (const t of all) this.onmessage?.(t);
  }
}

async function twoClients() {
  const hub = new FakeHub();
  const sa = hub.connect();
  const sb = hub.connect();
  const compiler = new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob'));
  const a = new DaemonBackend(sa, compiler);
  const b = new DaemonBackend(sb, compiler);
  const ma = new ProjectMirror(a);
  const mb = new ProjectMirror(b);
  await ma.open();
  await mb.open();
  return { hub, sa, sb, ma, mb };
}

const settle = () => new Promise((r) => setTimeout(r, 5));

describe('ProjectMirror over the daemon wire', () => {
  it('keeps a second client in step from state.changed pushes without refetching', async () => {
    const { ma, mb } = await twoClients();
    const id = counterIds();
    const [t, r] = [id(), id()];
    await ma.apply({ command: 'add_track', id: t, name: 'A', kind: 'instrument', index: null });
    await ma.apply({ command: 'add_region', id: r, track: t, start: 0, length: 3840, kind: 'midi' });
    await ma.apply({ command: 'add_note', id: id(), region: r, pitch: 60, velocity: 90, start: 0, duration: 480 });
    await ma.undo();
    await ma.redo();
    await settle();
    expect(mb.hash).toBe(ma.hash);
    expect(await mb.verify()).toBe(true);
    expect(mb.stats.fullFetches).toBe(1);
    expect(mb.stats.fallbacks).toBe(0);
    // The editing client sees its own edit twice (reply and push) and applies it once.
    expect(ma.stats.duplicates).toBeGreaterThan(0);
    expect(ma.stats.fullFetches).toBe(1);
  });

  it('asks for the gap with changes.since when a push was missed', async () => {
    const { sb, ma, mb } = await twoClients();
    const id = counterIds();
    sb.muted = true;
    await ma.apply({ command: 'add_track', id: id(), name: 'A', kind: 'instrument', index: null });
    await ma.apply({ command: 'add_track', id: id(), name: 'B', kind: 'instrument', index: null });
    // B never hears the first push, so the second names a base hash B does not hold.
    sb.dropHeld(1);
    sb.flush();
    await settle();
    expect(mb.hash).toBe(ma.hash);
    expect(mb.project?.tracks.length).toBe(2);
    expect(mb.stats.fullFetches).toBe(1);
    expect(mb.stats.fallbacks).toBe(0);
    expect(await mb.verify()).toBe(true);
  });

  it('fetches everything when the gap is no longer in the log', async () => {
    const { hub, sb, ma, mb } = await twoClients();
    const id = counterIds();
    sb.muted = true;
    await ma.apply({ command: 'add_track', id: id(), name: 'A', kind: 'instrument', index: null });
    await ma.apply({ command: 'add_track', id: id(), name: 'B', kind: 'instrument', index: null });
    sb.dropHeld(1);
    hub.log.length = 0;
    sb.flush();
    await settle();
    expect(mb.hash).toBe(ma.hash);
    expect(mb.stats.fallbacks).toBe(1);
    expect(await mb.verify()).toBe(true);
  });
});
