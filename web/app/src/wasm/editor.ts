// Typed wrapper over the ed_* exports of composer_wasm.wasm (see the composer-wasm rustdoc).
// Every call that returns bytes fills one result buffer owned by the handle, so it is copied
// out before the next call can invalidate it. Replies match the daemon's JSON-RPC result and
// error shapes without the envelope.
import type { Command, CommandInfo, Project } from '../model';

interface EditorExports {
  memory: WebAssembly.Memory;
  cw_alloc(len: number): number;
  cw_dealloc(ptr: number, len: number): void;
  ed_new(ptr: number, len: number): number;
  ed_free(h: number): void;
  ed_result_len(h: number): number;
  ed_apply_json(h: number, ptr: number, len: number): number;
  ed_group_json(h: number, ptr: number, len: number): number;
  ed_load_json(h: number, ptr: number, len: number): number;
  ed_transport_get_json(h: number): number;
  ed_transport_set_json(h: number, ptr: number, len: number): number;
  ed_undo(h: number): number;
  ed_redo(h: number): number;
  ed_undo_steps(h: number, steps: number): number;
  ed_redo_steps(h: number, steps: number): number;
  ed_history_json(h: number, ptr: number, len: number): number;
  ed_group_begin_json(h: number, ptr: number, len: number): number;
  ed_group_commit_json(h: number, ptr: number, len: number): number;
  ed_group_rollback_json(h: number, ptr: number, len: number): number;
  ed_enabled_json(h: number, ptr: number, len: number): number;
  ed_state_json(h: number): number;
  ed_describe_json(h: number): number;
  ed_state_hash(h: number): bigint;
  /** Absent in modules built before D25. */
  ed_set_feed?(h: number, maxChanges: number): void;
  ed_engine_blob(h: number, sampleRate: number): number;
  ed_sound_bank_load?(h: number, idPtr: number, idLen: number, ptr: number, len: number): number;
  ed_scale_json?(h: number, ptr: number, len: number): number;
  ed_chords_json?(h: number, ptr: number, len: number): number;
  ed_pitch_json?(h: number, ptr: number, len: number): number;
  /** Absent in modules built before D27. */
  ed_proposal_post_json?(h: number, ptr: number, len: number): number;
  ed_proposal_list_json?(h: number, ptr: number, len: number): number;
  ed_proposal_resolve_json?(h: number, ptr: number, len: number): number;
  /** Absent in modules built before D28. */
  ed_templates_list_json?(h: number): number;
  ed_recipes_list_json?(h: number): number;
  ed_template_apply_json?(h: number, ptr: number, len: number): number;
  ed_recipe_plan_json?(h: number, ptr: number, len: number): number;
  ed_recipe_apply_json?(h: number, ptr: number, len: number): number;
}

/** Change feed as sent by the wasm editor and composerd (D25). */
export interface RawFeed {
  base_hash: string;
  hash: string;
  /** Absent when `full` is true. */
  list?: unknown[];
  full?: boolean;
}

export interface RawSummary {
  state_hash: string;
  undo_depth: number;
  redo_depth: number;
  undo_label: string | null;
  changed?: boolean;
  applied?: boolean;
  /** undo and redo with steps (D20). */
  steps_applied?: number;
  changes?: RawFeed;
}

/** history.get entry (D20), oldest first. */
export interface RawHistoryEntry {
  index: number;
  label: string;
  source: string;
  command_count: number;
}

export interface RawHistoryPage {
  entries: RawHistoryEntry[];
  cursor: number;
  total: number;
  offset: number;
  state_hash: string;
}

export interface RawEnabled {
  name: string;
  enabled: boolean;
  reason: string | null;
  changes?: RawFeed;
}

/** transport.get and transport.set result, daemon field names. */
export interface RawTransport {
  playhead_tick: number;
  playhead_seconds: number;
  playing: boolean;
  end_tick: number;
  loop: { on: boolean; start: number; end: number };
  metronome: { on: boolean };
  count_in_bars: number;
  follow: boolean;
  dry_run?: boolean;
}

/** transport.set parameters, daemon field names. */
export interface RawTransportPatch {
  tick?: number;
  loop?: { on?: boolean; start?: number; end?: number };
  metronome?: { on?: boolean };
  count_in_bars?: number;
  follow?: boolean;
  dry_run?: boolean;
}

export interface RawError {
  code: number;
  message: string;
  data: { code: string; index?: number };
}

export type RawReply<T> = { result: T; error?: undefined } | { error: RawError; result?: undefined };

const enc = new TextEncoder();
const dec = new TextDecoder();

export class WasmEditor {
  private readonly x: EditorExports;
  private handle: number;

  constructor(instance: WebAssembly.Instance, name = 'untitled') {
    this.x = instance.exports as unknown as EditorExports;
    const bytes = enc.encode(name);
    const ptr = this.x.cw_alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer).set(bytes, ptr);
    this.handle = this.x.ed_new(ptr, bytes.length);
    this.x.cw_dealloc(ptr, bytes.length);
    if (!this.handle) throw new Error('composer-wasm: invalid project name');
  }

  static fromModule(module: WebAssembly.Module, name?: string): WasmEditor {
    return new WasmEditor(new WebAssembly.Instance(module, {}), name);
  }

  static fromBytes(bytes: BufferSource, name?: string): WasmEditor {
    return WasmEditor.fromModule(new WebAssembly.Module(bytes), name);
  }

  private take(ptr: number): Uint8Array {
    if (!ptr) throw new Error('composer-wasm: editor call failed');
    const len = this.x.ed_result_len(this.handle);
    return new Uint8Array(this.x.memory.buffer, ptr, len).slice();
  }

  private json<T>(fn: (h: number, p: number, l: number) => number, value: unknown): T {
    const bytes = enc.encode(JSON.stringify(value));
    const ptr = this.x.cw_alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer).set(bytes, ptr);
    const out = fn(this.handle, ptr, bytes.length);
    this.x.cw_dealloc(ptr, bytes.length);
    return JSON.parse(dec.decode(this.take(out))) as T;
  }

  private plain<T>(fn: (h: number) => number): T {
    return JSON.parse(dec.decode(this.take(fn(this.handle)))) as T;
  }

  apply(command: Command): RawReply<RawSummary> {
    return this.json((h, p, l) => this.x.ed_apply_json(h, p, l), command);
  }

  /** Atomic list as one undo step. */
  group(label: string, commands: Command[]): RawReply<RawSummary> {
    return this.json((h, p, l) => this.x.ed_group_json(h, p, l), { label, commands });
  }

  undo(): RawReply<RawSummary> {
    return this.plain((h) => this.x.ed_undo(h));
  }

  redo(): RawReply<RawSummary> {
    return this.plain((h) => this.x.ed_redo(h));
  }

  /** Undo or redo several steps in one call (D20). */
  jump(undo: boolean, steps: number): RawReply<RawSummary> {
    return this.plain((h) => (undo ? this.x.ed_undo_steps(h, steps) : this.x.ed_redo_steps(h, steps)));
  }

  history(params: { limit?: number; offset?: number }): RawReply<RawHistoryPage> {
    return this.json((h, p, l) => this.x.ed_history_json(h, p, l), params);
  }

  groupBegin(label: string, source?: string): RawReply<RawSummary & { token: string }> {
    return this.json((h, p, l) => this.x.ed_group_begin_json(h, p, l), source ? { label, source } : { label });
  }

  groupClose(token: string, commit: boolean): RawReply<RawSummary> {
    const fn = commit ? this.x.ed_group_commit_json : this.x.ed_group_rollback_json;
    return this.json((h, p, l) => fn.call(this.x, h, p, l), { token });
  }

  enabled(context: { track?: string; region?: string }): RawReply<{ commands: RawEnabled[] }> {
    return this.json((h, p, l) => this.x.ed_enabled_json(h, p, l), { context });
  }

  state(): RawReply<{ state_hash: string; project: Project }> {
    return this.plain((h) => this.x.ed_state_json(h));
  }

  describe(): RawReply<{ commands: CommandInfo[] }> {
    return this.plain((h) => this.x.ed_describe_json(h));
  }

  transportGet(): RawReply<RawTransport> {
    return this.plain((h) => this.x.ed_transport_get_json(h));
  }

  transportSet(patch: RawTransportPatch): RawReply<RawTransport> {
    return this.json((h, p, l) => this.x.ed_transport_set_json(h, p, l), patch);
  }

  /** Replaces the project and clears history. */
  load(project: Project): RawReply<RawSummary> {
    return this.json((h, p, l) => this.x.ed_load_json(h, p, l), project);
  }

  /** Turns the change feed on for edits of up to `maxChanges` changes (0 turns it off). False when the module predates D25. */
  setFeed(maxChanges: number): boolean {
    if (!this.x.ed_set_feed) return false;
    this.x.ed_set_feed(this.handle, maxChanges);
    return true;
  }

  /** State hash as 16 hex digits, the same text the daemon sends. */
  get stateHash(): string {
    return BigInt.asUintN(64, this.x.ed_state_hash(this.handle)).toString(16).padStart(16, '0');
  }

  /** Hands a SoundFont 2 file to the engine as the bank of catalogue sound `id` (D37). 0 means loaded. */
  loadSoundBank(id: string, bytes: Uint8Array): number {
    const f = this.x.ed_sound_bank_load;
    if (!f) return 4;
    const idBytes = enc.encode(id);
    const idPtr = this.x.cw_alloc(idBytes.length);
    new Uint8Array(this.x.memory.buffer).set(idBytes, idPtr);
    const ptr = this.x.cw_alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer).set(bytes, ptr);
    const code = f(this.handle, idPtr, idBytes.length, ptr, bytes.length);
    this.x.cw_dealloc(ptr, bytes.length);
    this.x.cw_dealloc(idPtr, idBytes.length);
    return code;
  }

  /** Engine blob v1 bytes (a copy); throws for a zero sample rate. */
  engineBlob(sampleRate: number): Uint8Array {
    return this.take(this.x.ed_engine_blob(this.handle, sampleRate));
  }

  /** theory.scale (D23): params `{root, scale}`. Throws when the module predates D23. */
  scaleJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_scale_json;
    if (!f) throw new Error('composer-wasm: no ed_scale_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** theory.chords (D23b): params `{key:{root,scale}, octave?}`. */
  chordsJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_chords_json;
    if (!f) throw new Error('composer-wasm: no ed_chords_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** theory.pitch (D23b): params `{root, scale, pitch}`. */
  pitchJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_pitch_json;
    if (!f) throw new Error('composer-wasm: no ed_pitch_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** proposal.post (D27): params `{label, commands, note?, author?, dry_run?}`. */
  proposalPostJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_proposal_post_json;
    if (!f) throw new Error('composer-wasm: no ed_proposal_post_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** proposal.list (D27): params `{status?}`. */
  proposalListJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_proposal_list_json;
    if (!f) throw new Error('composer-wasm: no ed_proposal_list_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** proposal.resolve (D27): params `{id, action, dry_run?}`. */
  proposalResolveJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_proposal_resolve_json;
    if (!f) throw new Error('composer-wasm: no ed_proposal_resolve_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** templates.list (D28). */
  templatesListJson<T>(): RawReply<T> {
    const f = this.x.ed_templates_list_json;
    if (!f) throw new Error('composer-wasm: no ed_templates_list_json export');
    return this.plain((h) => f(h));
  }

  /** recipes.list (D28). */
  recipesListJson<T>(): RawReply<T> {
    const f = this.x.ed_recipes_list_json;
    if (!f) throw new Error('composer-wasm: no ed_recipes_list_json export');
    return this.plain((h) => f(h));
  }

  /** template.apply (D28): params `{template, seed?, dry_run?}`. */
  templateApplyJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_template_apply_json;
    if (!f) throw new Error('composer-wasm: no ed_template_apply_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** recipe.plan (D28): params `{recipe, params?, scope?, seed?}`. */
  recipePlanJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_recipe_plan_json;
    if (!f) throw new Error('composer-wasm: no ed_recipe_plan_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  /** recipe.apply (D28): params `{recipe, params?, scope?, seed?, dry_run?}`. */
  recipeApplyJson<T>(params: object): RawReply<T> {
    const f = this.x.ed_recipe_apply_json;
    if (!f) throw new Error('composer-wasm: no ed_recipe_apply_json export');
    return this.json<RawReply<T>>((h, p, l) => f(h, p, l), params);
  }

  free(): void {
    if (this.handle) this.x.ed_free(this.handle);
    this.handle = 0;
  }
}
