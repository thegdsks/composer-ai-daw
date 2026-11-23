// Backend over the in-page wasm editor: no server, the same commands the daemon runs.
import type { Command, CommandInfo, Project } from '../model';
import { builtinCatalogue } from '../sounds/builtin';
import type { PackLoader } from '../sounds/packs';
import { feedConfig } from './feedConfig';
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
import { WasmEditor, type RawFeed, type RawHistoryPage, type RawReply, type RawSummary, type RawTransport, type RawTransportPatch } from '../wasm/editor';
import {
  fail,
  type Backend,
  type Change,
  type ChangeFeed,
  type CommandEnabled,
  type EnabledContext,
  type HistoryPage,
  type StepSource,
  type FileResult,
  type Reply,
  type SoundCatalogue,
  type StateSnapshot,
  type Summary,
  type TransportPatch,
  type ProposalFilter,
  type ProposalList,
  type ProposalPost,
  type ProposalPosted,
  type TheoryChords,
  type TheoryKey,
  type TheoryPitch,
  type TheoryScale,
  type TransportState,
} from './types';

export function toFeed(f: RawFeed): ChangeFeed {
  return { baseHash: f.base_hash, hash: f.hash, changes: f.full || !f.list ? null : (f.list as Change[]) };
}

export function toSummary(s: RawSummary): Summary {
  return {
    stateHash: s.state_hash,
    undoDepth: s.undo_depth,
    redoDepth: s.redo_depth,
    undoLabel: s.undo_label,
    changed: s.changed,
    applied: s.applied,
    stepsApplied: s.steps_applied,
    ...(s.changes ? { feed: toFeed(s.changes) } : {}),
  };
}

export function toTransport(r: RawTransport): TransportState {
  return {
    playheadTick: r.playhead_tick,
    playheadSeconds: r.playhead_seconds,
    playing: r.playing,
    endTick: r.end_tick,
    loop: { ...r.loop },
    metronome: { ...r.metronome },
    countInBars: r.count_in_bars,
    follow: r.follow,
  };
}

export function toRawPatch(p: TransportPatch, dryRun = false): RawTransportPatch {
  const raw: RawTransportPatch = {};
  if (p.tick !== undefined) raw.tick = p.tick;
  if (p.loop !== undefined) raw.loop = p.loop;
  if (p.metronome !== undefined) raw.metronome = p.metronome;
  if (p.countInBars !== undefined) raw.count_in_bars = p.countInBars;
  if (p.follow !== undefined) raw.follow = p.follow;
  if (dryRun) raw.dry_run = true;
  return raw;
}

export function toHistoryPage(r: RawHistoryPage): HistoryPage {
  return {
    entries: r.entries.map((e) => ({
      index: e.index,
      label: e.label,
      source: toSource(e.source),
      commandCount: e.command_count,
    })),
    cursor: r.cursor,
    total: r.total,
    offset: r.offset,
  };
}

function toSource(s: string): StepSource {
  return s === 'ai' || s === 'template' || s === 'recipe' ? s : 'user';
}

export function unwrap<T, U>(r: RawReply<T>, map: (v: T) => U): Reply<U> {
  if (r.error) {
    return { ok: false, error: { code: r.error.data.code, message: r.error.message, index: r.error.data.index } };
  }
  return { ok: true, value: map(r.result) };
}

export class WasmBackend implements Backend {
  readonly kind = 'wasm';
  readonly hasFiles = false;
  private undoDepth = 0;
  private redoDepth = 0;

  constructor(
    private readonly editor: WasmEditor,
    private readonly packs: PackLoader | null = null,
  ) {
    editor.setFeed(feedConfig.maxChanges);
  }

  static create(module: WebAssembly.Module, name = 'untitled', packs: PackLoader | null = null): WasmBackend {
    return new WasmBackend(WasmEditor.fromModule(module, name), packs);
  }

  private track(r: Reply<Summary>): Reply<Summary> {
    if (r.ok) {
      this.undoDepth = r.value.undoDepth;
      this.redoDepth = r.value.redoDepth;
    }
    return r;
  }

  async apply(command: Command): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.apply(command), toSummary));
  }

  async group(label: string, commands: Command[]): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.group(label, commands), toSummary));
  }

  async undo(): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.undo(), toSummary));
  }

  async redo(): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.redo(), toSummary));
  }

  async state(): Promise<Reply<StateSnapshot>> {
    return unwrap(this.editor.state(), (s) => ({
      stateHash: s.state_hash,
      project: s.project,
      undoDepth: this.undoDepth,
      redoDepth: this.redoDepth,
    }));
  }

  async describe(): Promise<Reply<CommandInfo[]>> {
    return unwrap(this.editor.describe(), (d) => d.commands);
  }

  async engineBlob(sampleRate: number): Promise<Reply<Uint8Array>> {
    try {
      // Sampled sounds in use are loaded into the engine before it compiles the blob.
      if (this.packs) {
        const snap = this.editor.state();
        const used = new Set((snap.result?.project.tracks ?? []).map((t) => t.sound).filter((s): s is string => !!s));
        for (const id of used) await this.packs.ensure(this.editor, id);
      }
      return { ok: true, value: this.editor.engineBlob(sampleRate) };
    } catch (e) {
      return fail('blob_failed', e instanceof Error ? e.message : 'blob failed');
    }
  }

  async load(project: Project): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.load(project), toSummary));
  }

  async saveProject(): Promise<Reply<FileResult>> {
    return fail('unsupported', 'the in-browser editor saves through a download');
  }

  async openProject(): Promise<Reply<Summary & FileResult>> {
    return fail('unsupported', 'the in-browser editor opens files through the file picker');
  }

  /** No catalogue file lives in the page: the built in voices plus any fetched packs the page serves. */
  async sounds(): Promise<Reply<SoundCatalogue>> {
    const cat = builtinCatalogue();
    if (this.packs) cat.entries.push(...(await this.packs.entries()));
    return { ok: true, value: cat };
  }

  async transportGet(): Promise<Reply<TransportState>> {
    return unwrap(this.editor.transportGet(), toTransport);
  }

  async transportSet(patch: TransportPatch, dryRun = false): Promise<Reply<TransportState>> {
    return unwrap(this.editor.transportSet(toRawPatch(patch, dryRun)), toTransport);
  }

  async history(offset: number, limit?: number): Promise<Reply<HistoryPage>> {
    return unwrap(this.editor.history(limit === undefined ? { offset } : { offset, limit }), toHistoryPage);
  }

  async jump(direction: 'undo' | 'redo', steps: number): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.jump(direction === 'undo', steps), toSummary));
  }

  async groupBegin(label: string, source?: StepSource): Promise<Reply<{ token: string }>> {
    return unwrap(this.editor.groupBegin(label, source), (s) => ({ token: s.token }));
  }

  async groupCommit(token: string): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.groupClose(token, true), toSummary));
  }

  async groupRollback(token: string): Promise<Reply<Summary>> {
    return this.track(unwrap(this.editor.groupClose(token, false), toSummary));
  }

  async enabled(context: EnabledContext = {}): Promise<Reply<CommandEnabled[]>> {
    return unwrap(this.editor.enabled(context), (r) => r.commands);
  }

  async theoryScale(key: TheoryKey): Promise<Reply<TheoryScale>> {
    try {
      return unwrap(this.editor.scaleJson<RawTheoryScale>({ root: key.root, scale: key.scale }), toTheoryScale);
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'theory.scale failed');
    }
  }

  async theoryChords(key: TheoryKey, octave?: number): Promise<Reply<TheoryChords>> {
    try {
      const params = { key: { root: key.root, scale: key.scale }, ...(octave === undefined ? {} : { octave }) };
      return unwrap(this.editor.chordsJson<RawTheoryChords>(params), toTheoryChords);
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'theory.chords failed');
    }
  }

  async theoryPitch(key: TheoryKey, pitch: number): Promise<Reply<TheoryPitch>> {
    try {
      return unwrap(
        this.editor.pitchJson<RawTheoryPitch>({ root: key.root, scale: key.scale, pitch }),
        toTheoryPitch,
      );
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'theory.pitch failed');
    }
  }

  async proposalPost(post: ProposalPost): Promise<Reply<ProposalPosted>> {
    try {
      return unwrap(this.editor.proposalPostJson<RawProposalPosted>(toRawPost(post)), toProposalPosted);
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'proposal.post failed');
    }
  }

  async proposalList(status?: ProposalFilter): Promise<Reply<ProposalList>> {
    try {
      return unwrap(
        this.editor.proposalListJson<RawProposalList>(status === undefined ? {} : { status }),
        toProposalList,
      );
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'proposal.list failed');
    }
  }

  async proposalResolve(id: string, action: 'accept' | 'reject', dryRun = false): Promise<Reply<Summary>> {
    try {
      const raw = this.editor.proposalResolveJson<RawSummary>({ id, action, ...(dryRun ? { dry_run: true } : {}) });
      return this.track(unwrap(raw, toSummary));
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'proposal.resolve failed');
    }
  }

  async templatesList(): Promise<Reply<TemplateInfo[]>> {
    try {
      return unwrap(this.editor.templatesListJson<{ templates: RawTemplateInfo[] }>(), toTemplates);
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'templates.list failed');
    }
  }

  async templateApply(template: string, seed?: number): Promise<Reply<Summary>> {
    try {
      const raw = this.editor.templateApplyJson<RawSummary>({ template, ...(seed === undefined ? {} : { seed }) });
      return this.track(unwrap(raw, toSummary));
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'template.apply failed');
    }
  }

  async recipesList(): Promise<Reply<RecipeInfo[]>> {
    try {
      return unwrap(this.editor.recipesListJson<{ recipes: RecipeInfo[] }>(), toRecipes);
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'recipes.list failed');
    }
  }

  async recipePlan(request: RecipeRequest): Promise<Reply<RecipePlan>> {
    try {
      return unwrap(this.editor.recipePlanJson<RawRecipePlan>(toRawRecipe(request)), toRecipePlan);
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'recipe.plan failed');
    }
  }

  async recipeApply(request: RecipeRequest): Promise<Reply<Summary>> {
    try {
      return this.track(unwrap(this.editor.recipeApplyJson<RawSummary>(toRawRecipe(request)), toSummary));
    } catch (e) {
      return fail('unsupported', e instanceof Error ? e.message : 'recipe.apply failed');
    }
  }

  /** Nobody else edits an in-page session, so there is nothing to listen to. */
  subscribe(): () => void {
    return () => undefined;
  }

  dispose(): void {
    this.editor.free();
  }
}
