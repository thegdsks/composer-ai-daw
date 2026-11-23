// Backend over composerd: JSON-RPC 2.0 on a localhost WebSocket (decision D10). The daemon
// has no audio output a browser can use, so playback still runs on the wasm engine: the
// project JSON from the daemon is loaded into a local wasm editor, which compiles the blob.
import type { Command, CommandInfo, Project } from '../model';
import { config } from '../config';
import { WasmEditor, type RawFeed, type RawHistoryPage, type RawSummary, type RawTransport } from '../wasm/editor';
import { toFeed, toHistoryPage, toRawPatch, toSummary, toTransport } from './wasmBackend';
import {
  toProposalList,
  toProposalPosted,
  toRawPost,
  type RawProposalList,
  type RawProposalPosted,
} from './proposals';
import {
  toRawRecipe,
  toRecipePlan,
  toRecipes,
  toTemplates,
  type RawRecipePlan,
  type RawTemplateInfo,
  type RecipeInfo,
  type RecipePlan,
  type RecipeRequest,
  type TemplateInfo,
} from './recipes';
import {
  toTheoryChords,
  toTheoryPitch,
  toTheoryScale,
  type RawTheoryChords,
  type RawTheoryPitch,
  type RawTheoryScale,
} from './theory';
import {
  fail,
  type Backend,
  type BackendError,
  type CommandEnabled,
  type EnabledContext,
  type HistoryPage,
  type StepSource,
  type ChangeFeed,
  type FileResult,
  type Reply,
  type SoundCatalogue,
  type StateChange,
  type StateSnapshot,
  type Summary,
  type ProposalFilter,
  type ProposalList,
  type ProposalPost,
  type ProposalPosted,
  type TheoryChords,
  type TheoryKey,
  type TheoryPitch,
  type TheoryScale,
  type TransportPatch,
  type TransportState,
} from './types';

/** The part of WebSocket the backend needs, so tests can supply a fake. */
export interface RpcSocket {
  send(data: string): void;
  close(): void;
  onmessage: ((data: string) => void) | null;
  onclose: (() => void) | null;
}

export interface BlobCompiler {
  compile(project: Project, sampleRate: number): Uint8Array;
}

/** Compiles a project to an engine blob with a private wasm editor (state to blob via load). */
export class WasmBlobCompiler implements BlobCompiler {
  constructor(private readonly editor: WasmEditor) {}

  compile(project: Project, sampleRate: number): Uint8Array {
    const r = this.editor.load(project);
    if (r.error) throw new Error(r.error.message);
    return this.editor.engineBlob(sampleRate);
  }
}

interface RpcError {
  code: number;
  message: string;
  data?: { code?: string; index?: number };
}

type Pending = (r: { result: unknown } | { error: BackendError }) => void;

/** Reads `token=` from a URL hash such as `#token=abc`. */
export function tokenFromHash(hash: string): string | null {
  const params = new URLSearchParams(hash.replace(/^#/, ''));
  return params.get('token');
}

export function openWebSocket(url: string, token: string): Promise<RpcSocket> {
  return new Promise((resolve, reject) => {
    // Browsers cannot set headers on a WebSocket, so the token rides in the query.
    const u = new URL(url);
    u.searchParams.set('token', token);
    const ws = new WebSocket(u);
    const sock: RpcSocket = {
      send: (d) => ws.send(d),
      close: () => ws.close(),
      onmessage: null,
      onclose: null,
    };
    ws.onmessage = (e) => sock.onmessage?.(String(e.data));
    ws.onclose = () => sock.onclose?.();
    ws.onopen = () => resolve(sock);
    ws.onerror = () => reject(new Error('daemon connection failed'));
  });
}

export class DaemonBackend implements Backend {
  readonly kind = 'daemon';
  readonly hasFiles = true;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly listeners = new Set<(c: StateChange) => void>();
  private cache: { hash: string; sampleRate: number; blob: Uint8Array } | null = null;
  private closed = false;

  constructor(
    private readonly socket: RpcSocket,
    private readonly compiler: BlobCompiler,
  ) {
    socket.onmessage = (data) => this.onMessage(data);
    socket.onclose = () => this.onClose();
  }

  /** Opens the socket and subscribes to state.changed. */
  static async connect(url: string, token: string, compiler: BlobCompiler): Promise<DaemonBackend> {
    const backend = new DaemonBackend(await openWebSocket(url, token), compiler);
    const sub = await backend.call('subscribe', { enabled: true });
    if (!sub.ok) throw new Error(sub.error.message);
    return backend;
  }

  private onMessage(data: string): void {
    let msg: {
      id?: number | null;
      result?: unknown;
      error?: RpcError;
      method?: string;
      params?: { hash: string; cause: string; changes?: RawFeed };
    };
    try {
      msg = JSON.parse(data) as typeof msg;
    } catch {
      return;
    }
    if (msg.method === 'state.changed' && msg.params) {
      const change: StateChange = { hash: msg.params.hash, cause: msg.params.cause };
      if (msg.params.changes) change.feed = toFeed(msg.params.changes);
      for (const l of this.listeners) l(change);
      return;
    }
    if (typeof msg.id !== 'number') return;
    const done = this.pending.get(msg.id);
    if (!done) return;
    this.pending.delete(msg.id);
    if (msg.error) {
      done({
        error: {
          code: msg.error.data?.code ?? 'rpc_error',
          message: msg.error.message,
          index: msg.error.data?.index,
        },
      });
    } else {
      done({ result: msg.result });
    }
  }

  private onClose(): void {
    this.closed = true;
    for (const done of this.pending.values()) {
      done({ error: { code: 'connection_closed', message: 'daemon connection closed' } });
    }
    this.pending.clear();
  }

  private call<T>(method: string, params: object = {}): Promise<Reply<T>> {
    if (this.closed) return Promise.resolve(fail('connection_closed', 'daemon connection closed'));
    const id = this.nextId++;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        resolve(fail('timeout', `${method} timed out`));
      }, config.rpcTimeoutMs);
      this.pending.set(id, (r) => {
        clearTimeout(timer);
        resolve('error' in r ? { ok: false, error: r.error } : { ok: true, value: r.result as T });
      });
      this.socket.send(JSON.stringify({ jsonrpc: '2.0', id, method, params }));
    });
  }

  private async summary(method: string, params: object): Promise<Reply<Summary>> {
    const r = await this.call<RawSummary>(method, params);
    return r.ok ? { ok: true, value: toSummary(r.value) } : r;
  }

  apply(command: Command): Promise<Reply<Summary>> {
    return this.summary('command.apply', { command, changes: true });
  }

  group(label: string, commands: Command[]): Promise<Reply<Summary>> {
    return this.summary('command.group', { label, commands, changes: true });
  }

  undo(): Promise<Reply<Summary>> {
    return this.summary('undo', { changes: true });
  }

  redo(): Promise<Reply<Summary>> {
    return this.summary('redo', { changes: true });
  }

  async state(): Promise<Reply<StateSnapshot>> {
    // A dry run undo reports the history depths without changing anything.
    const [s, d] = await Promise.all([
      this.call<{ state_hash: string; project: Project }>('state.get'),
      this.call<RawSummary>('undo', { dry_run: true }),
    ]);
    if (!s.ok) return s;
    return {
      ok: true,
      value: {
        stateHash: s.value.state_hash,
        project: s.value.project,
        undoDepth: d.ok ? d.value.undo_depth : 0,
        redoDepth: d.ok ? d.value.redo_depth : 0,
      },
    };
  }

  async describe(): Promise<Reply<CommandInfo[]>> {
    const r = await this.call<{ commands: CommandInfo[] }>('commands.describe');
    return r.ok ? { ok: true, value: r.value.commands } : r;
  }

  async engineBlob(sampleRate: number): Promise<Reply<Uint8Array>> {
    const st = await this.state();
    if (!st.ok) return st;
    const c = this.cache;
    if (c && c.hash === st.value.stateHash && c.sampleRate === sampleRate) {
      return { ok: true, value: c.blob };
    }
    try {
      const blob = this.compiler.compile(st.value.project, sampleRate);
      this.cache = { hash: st.value.stateHash, sampleRate, blob };
      return { ok: true, value: blob };
    } catch (e) {
      return fail('blob_failed', e instanceof Error ? e.message : 'blob failed');
    }
  }

  /** composerd loads files only (project.load); JSON from the browser has no method. */
  async load(): Promise<Reply<Summary>> {
    return fail('unsupported', 'composerd opens project files by name, not from JSON');
  }

  async saveProject(path: string, overwrite: boolean): Promise<Reply<FileResult>> {
    return this.call<FileResult>('project.save', { path, overwrite });
  }

  async openProject(path: string): Promise<Reply<Summary & FileResult>> {
    const r = await this.call<RawSummary & FileResult>('project.load', { path });
    if (!r.ok) return r;
    const { path: p, name, tracks } = r.value;
    return { ok: true, value: { ...toSummary(r.value), path: p, name, tracks } };
  }

  async sounds(): Promise<Reply<SoundCatalogue>> {
    const r = await this.call<{ entries: SoundCatalogue['entries']; gaps?: string[] }>('sounds.list');
    return r.ok ? { ok: true, value: { entries: r.value.entries, gaps: r.value.gaps ?? [] } } : r;
  }

  async transportGet(): Promise<Reply<TransportState>> {
    const r = await this.call<RawTransport>('transport.get');
    return r.ok ? { ok: true, value: toTransport(r.value) } : r;
  }

  async transportSet(patch: TransportPatch, dryRun = false): Promise<Reply<TransportState>> {
    const r = await this.call<RawTransport>('transport.set', toRawPatch(patch, dryRun));
    return r.ok ? { ok: true, value: toTransport(r.value) } : r;
  }

  async history(offset: number, limit?: number): Promise<Reply<HistoryPage>> {
    const r = await this.call<RawHistoryPage>('history.get', limit === undefined ? { offset } : { offset, limit });
    return r.ok ? { ok: true, value: toHistoryPage(r.value) } : r;
  }

  jump(direction: 'undo' | 'redo', steps: number): Promise<Reply<Summary>> {
    return this.summary(direction, { steps });
  }

  async groupBegin(label: string, source?: StepSource): Promise<Reply<{ token: string }>> {
    return this.call<{ token: string }>('group.begin', source ? { label, source } : { label });
  }

  groupCommit(token: string): Promise<Reply<Summary>> {
    return this.summary('group.commit', { token });
  }

  groupRollback(token: string): Promise<Reply<Summary>> {
    return this.summary('group.rollback', { token });
  }

  async enabled(context: EnabledContext = {}): Promise<Reply<CommandEnabled[]>> {
    const r = await this.call<{ commands: CommandEnabled[] }>('commands.enabled', { context });
    return r.ok ? { ok: true, value: r.value.commands } : r;
  }

  async theoryScale(key: TheoryKey): Promise<Reply<TheoryScale>> {
    const r = await this.call<RawTheoryScale>('theory.scale', { root: key.root, scale: key.scale });
    return r.ok ? { ok: true, value: toTheoryScale(r.value) } : r;
  }

  async theoryChords(key: TheoryKey, octave?: number): Promise<Reply<TheoryChords>> {
    const params = { key: { root: key.root, scale: key.scale }, ...(octave === undefined ? {} : { octave }) };
    const r = await this.call<RawTheoryChords>('theory.chords', params);
    return r.ok ? { ok: true, value: toTheoryChords(r.value) } : r;
  }

  async theoryPitch(key: TheoryKey, pitch: number): Promise<Reply<TheoryPitch>> {
    const r = await this.call<RawTheoryPitch>('theory.pitch', { key: { root: key.root, scale: key.scale }, pitch });
    return r.ok ? { ok: true, value: toTheoryPitch(r.value) } : r;
  }

  async proposalPost(post: ProposalPost): Promise<Reply<ProposalPosted>> {
    const r = await this.call<RawProposalPosted>('proposal.post', toRawPost(post));
    return r.ok ? { ok: true, value: toProposalPosted(r.value) } : r;
  }

  async proposalList(status?: ProposalFilter): Promise<Reply<ProposalList>> {
    const r = await this.call<RawProposalList>('proposal.list', status === undefined ? {} : { status });
    return r.ok ? { ok: true, value: toProposalList(r.value) } : r;
  }

  proposalResolve(id: string, action: 'accept' | 'reject', dryRun = false): Promise<Reply<Summary>> {
    return this.summary('proposal.resolve', { id, action, ...(dryRun ? { dry_run: true } : {}) });
  }

  async templatesList(): Promise<Reply<TemplateInfo[]>> {
    const r = await this.call<{ templates: RawTemplateInfo[] }>('templates.list');
    return r.ok ? { ok: true, value: toTemplates(r.value) } : r;
  }

  templateApply(template: string, seed?: number): Promise<Reply<Summary>> {
    return this.summary('template.apply', { template, ...(seed === undefined ? {} : { seed }), changes: true });
  }

  async recipesList(): Promise<Reply<RecipeInfo[]>> {
    const r = await this.call<{ recipes: RecipeInfo[] }>('recipes.list');
    return r.ok ? { ok: true, value: toRecipes(r.value) } : r;
  }

  async recipePlan(request: RecipeRequest): Promise<Reply<RecipePlan>> {
    const r = await this.call<RawRecipePlan>('recipe.plan', toRawRecipe(request));
    return r.ok ? { ok: true, value: toRecipePlan(r.value) } : r;
  }

  recipeApply(request: RecipeRequest): Promise<Reply<Summary>> {
    return this.summary('recipe.apply', { ...toRawRecipe(request), changes: true });
  }

  async changesSince(hash: string): Promise<Reply<ChangeFeed>> {
    const r = await this.call<RawFeed>('changes.since', { hash });
    return r.ok ? { ok: true, value: toFeed(r.value) } : r;
  }

  subscribe(listener: (c: StateChange) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  dispose(): void {
    this.listeners.clear();
    this.socket.close();
  }
}
