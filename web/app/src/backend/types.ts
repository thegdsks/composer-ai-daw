// The only surface the UI talks to. Two implementations: WasmBackend (editor inside the
// page) and DaemonBackend (JSON-RPC to composerd). Every call resolves, never throws for a
// rejected command: failures come back as `{ ok: false }` with the stable error code.
import type { Command, CommandInfo, Id, Note, Project, Region, ScaleName, TimeSignature, Track } from '../model';
import type { ProposalFilter, ProposalList, ProposalPost, ProposalPosted } from './proposals';
import type { RecipeInfo, RecipePlan, RecipeRequest, TemplateInfo } from './recipes';
import type { TheoryChords, TheoryKey, TheoryPitch, TheoryScale } from './theory';

export type {
  Proposal,
  ProposalFilter,
  ProposalList,
  ProposalPost,
  ProposalPosted,
  ProposalStatus,
} from './proposals';
export type {
  RecipeInfo,
  RecipeParam,
  RecipePlan,
  RecipeRequest,
  RecipeScope,
  TemplateInfo,
} from './recipes';
export type { ChordQuality, TheoryChord, TheoryChords, TheoryKey, TheoryPitch, TheoryScale } from './theory';

/** One low level change record as the core sends it (composer-model Change, op in snake_case). */
export type Change =
  | { op: 'new_track'; index: number; track: Track }
  | { op: 'delete_track'; index: number; track: Track }
  | { op: 'new_region'; track: Id; region: Region }
  | { op: 'delete_region'; track: Id; region: Region }
  | { op: 'new_note'; region: Id; note: Note }
  | { op: 'delete_note'; region: Id; note: Note }
  | { op: 'set'; key: FieldKey; old: FieldValue; new: FieldValue }
  | { op: 'set_tempo'; tick: number; old: number | null; new: number | null }
  | { op: 'set_meter'; tick: number; old: TimeSignature | null; new: TimeSignature | null }
  | { op: 'set_key'; tick: number; old: KeyEntry | null; new: KeyEntry | null }
  | { op: 'set_marker'; tick: number; old: string | null; new: string | null }
  /** Effect chain edits (D45). The mirror does not replay them: it falls back to one full fetch. */
  | { op: 'insert_effect' | 'remove_effect' | 'move_effect' | 'set_effect_bypass' | 'set_effect_param' };

export interface KeyEntry {
  root: number;
  scale: ScaleName;
}

export type FieldKey =
  | { field: 'project_name' }
  | { field: 'track_name' | 'track_instrument' | 'track_mix' | 'track_sound'; track: Id }
  | { field: 'region_start' | 'region_length' | 'region_loop'; region: Id }
  | { field: 'note_pitch' | 'note_velocity' | 'note_start' | 'note_duration'; region: Id; note: Id };

export type FieldValue =
  | { type: 'text'; value: string }
  | { type: 'slot'; value: Id | null }
  | { type: 'mix'; value: { volume: number; pan: number; mute: boolean; solo: boolean } }
  | { type: 'sound'; value: string | null }
  | { type: 'tick'; value: number }
  | { type: 'byte'; value: number };

/**
 * What one edit did (D25). `changes` is null when the edit was too large to list or replaced
 * the session: the client must fetch the full state. `baseHash` is the state the changes
 * apply to and `hash` the state after them.
 */
export interface ChangeFeed {
  baseHash: string;
  hash: string;
  changes: Change[] | null;
}

export interface Summary {
  stateHash: string;
  undoDepth: number;
  redoDepth: number;
  undoLabel: string | null;
  /** Apply only: false when the command was a no-op. */
  changed?: boolean;
  /** Undo and redo only: false when there was nothing to do. */
  applied?: boolean;
  /** Undo and redo with a step count: how many steps moved. */
  stepsApplied?: number;
  /** The primitive changes of this edit (D25), when the backend sends them. */
  feed?: ChangeFeed;
}

export interface StateSnapshot {
  stateHash: string;
  project: Project;
  undoDepth: number;
  redoDepth: number;
}

export interface BackendError {
  /** Stable machine readable reason, for example `not_found` or `invalid`. */
  code: string;
  message: string;
  /** Index of the failing command inside a group. */
  index?: number;
}

export type Reply<T> = { ok: true; value: T } | { ok: false; error: BackendError };

export interface StateChange {
  hash: string;
  cause: string;
  /** The change feed of the edit behind this push (D25), when the daemon sent one. */
  feed?: ChangeFeed;
}

/** One entry of the sound catalogue (composer-catalogue, scalar fields only). */
export interface SoundEntry {
  id: string;
  name: string;
  kind: string;
  licence: string;
  instruments: string;
  /** `yes`, `maybe` or `no`: whether the entry may ship inside the app. */
  bundle: string;
  /** Fields the daemon sends in full; the in-page list leaves some out. */
  url?: string;
  attribution_required?: string;
  attribution_text?: string | null;
  approx_size?: string;
  /** `listed`, `fetched` or `wired`; only wired sampled sounds can play. */
  status?: string;
  /** Path of the pack file under the served packs folder. */
  file?: string | null;
}

export interface SoundCatalogue {
  entries: SoundEntry[];
  gaps: string[];
}

export interface FileResult {
  path: string;
  bytes?: number;
  name?: string;
  tracks?: number;
}

/** Loop range in ticks. */
export interface LoopState {
  on: boolean;
  start: number;
  end: number;
}

/** Transport session state (D21): saved with the project file, not undoable, not in the state hash. */
export interface TransportPrefs {
  loop: LoopState;
  metronome: { on: boolean };
  countInBars: number;
  follow: boolean;
}

export interface TransportState extends TransportPrefs {
  playheadTick: number;
  playheadSeconds: number;
  /** Always false until a realtime host drives the transport (D17). */
  playing: boolean;
  endTick: number;
}

/** Every field optional, merged onto the current value; at least one is required. */
export interface TransportPatch {
  tick?: number;
  loop?: Partial<LoopState>;
  metronome?: { on?: boolean };
  countInBars?: number;
  follow?: boolean;
}

/** Who made a history step (D20). */
export type StepSource = 'user' | 'ai' | 'template' | 'recipe';

export interface HistoryEntry {
  /** Position in the whole history, 0 is the oldest step. */
  index: number;
  label: string;
  source: StepSource;
  commandCount: number;
}

/** A page of history, oldest first: applied steps up to the cursor, then undone ones in redo order. */
export interface HistoryPage {
  entries: HistoryEntry[];
  /** Count of applied steps. */
  cursor: number;
  total: number;
  offset: number;
}

/** Whether a command can run now (D33); `reason` is a plain sentence when it cannot. */
export interface CommandEnabled {
  name: string;
  enabled: boolean;
  reason: string | null;
}

export interface EnabledContext {
  track?: Id;
  region?: Id;
}

export interface Backend {
  readonly kind: 'wasm' | 'daemon';
  /** True when the backend keeps project files itself (daemon files dir) via saveProject and openProject. */
  readonly hasFiles: boolean;
  apply(command: Command): Promise<Reply<Summary>>;
  /** Atomic list of commands, one undo step. */
  group(label: string, commands: Command[]): Promise<Reply<Summary>>;
  undo(): Promise<Reply<Summary>>;
  redo(): Promise<Reply<Summary>>;
  state(): Promise<Reply<StateSnapshot>>;
  describe(): Promise<Reply<CommandInfo[]>>;
  /** Compiled engine blob for the current project (see the composer-wasm rustdoc). */
  engineBlob(sampleRate: number): Promise<Reply<Uint8Array>>;
  /** Replaces the whole project from JSON and clears history; the daemon only opens files (openProject). */
  load(project: Project): Promise<Reply<Summary>>;
  /** Writes the project into the daemon files dir (relative name). Fails `unsupported` without files. */
  saveProject(path: string, overwrite: boolean): Promise<Reply<FileResult>>;
  /** Replaces the session with a project file from the daemon files dir; history is cleared. */
  openProject(path: string): Promise<Reply<Summary & FileResult>>;
  /** The sound catalogue. Fails `unsupported` when the backend has none (the UI hides the slot). */
  sounds(): Promise<Reply<SoundCatalogue>>;
  /** Playhead plus loop, metronome, count in and follow (transport.get). */
  transportGet(): Promise<Reply<TransportState>>;
  /** Merges the given fields; `dryRun` validates and reports without changing anything. */
  transportSet(patch: TransportPatch, dryRun?: boolean): Promise<Reply<TransportState>>;
  /** One page of the undo history (history.get). */
  history(offset: number, limit?: number): Promise<Reply<HistoryPage>>;
  /** Undo or redo several steps at once; fewer move when history runs out (see stepsApplied). */
  jump(direction: 'undo' | 'redo', steps: number): Promise<Reply<Summary>>;
  /** Opens the one allowed group; edits join it live until commit or rollback (D32). */
  groupBegin(label: string, source?: StepSource): Promise<Reply<{ token: string }>>;
  groupCommit(token: string): Promise<Reply<Summary>>;
  groupRollback(token: string): Promise<Reply<Summary>>;
  /** Availability of every command now, with a plain reason when not (commands.enabled). */
  enabled(context?: EnabledContext): Promise<Reply<CommandEnabled[]>>;
  /** Changes since the state with this hash (D25). Optional: only the daemon keeps a log. */
  changesSince?(hash: string): Promise<Reply<ChangeFeed>>;
  /** Scale pitch classes of a key (theory.scale). Optional so older stand ins still type check. */
  theoryScale?(key: TheoryKey): Promise<Reply<TheoryScale>>;
  /** Diatonic triads and sevenths of a key, default octave 4 (theory.chords). */
  theoryChords?(key: TheoryKey, octave?: number): Promise<Reply<TheoryChords>>;
  /** Key membership, degree and nearest in key pitch of a MIDI pitch (theory.pitch). */
  theoryPitch?(key: TheoryKey, pitch: number): Promise<Reply<TheoryPitch>>;
  /** Posts a proposal that changes nothing until a person accepts it (proposal.post, D27). Optional: older stand ins. */
  proposalPost?(post: ProposalPost): Promise<Reply<ProposalPosted>>;
  /** Lists proposals, pending by default (proposal.list). */
  proposalList?(status?: ProposalFilter): Promise<Reply<ProposalList>>;
  /** Accepts (one group with source ai, one undo step) or rejects a proposal. A stale one fails with the command error and stays pending. */
  proposalResolve?(id: string, action: 'accept' | 'reject', dryRun?: boolean): Promise<Reply<Summary>>;
  /** The bundled starter templates (templates.list, D28). Optional: older stand ins. */
  templatesList?(): Promise<Reply<TemplateInfo[]>>;
  /** Applies a template as one group with source template (template.apply). */
  templateApply?(template: string, seed?: number): Promise<Reply<Summary>>;
  /** Recipes with their typed parameters (recipes.list). */
  recipesList?(): Promise<Reply<RecipeInfo[]>>;
  /** The command list a recipe would write and what it would change; applies nothing (recipe.plan). */
  recipePlan?(request: RecipeRequest): Promise<Reply<RecipePlan>>;
  /** Applies a recipe as one group with source recipe (recipe.apply). */
  recipeApply?(request: RecipeRequest): Promise<Reply<Summary>>;
  /** Calls back when another client changes the state. Returns an unsubscribe function. */
  subscribe(listener: (change: StateChange) => void): () => void;
  dispose(): void;
}

export function fail(code: string, message: string): { ok: false; error: BackendError } {
  return { ok: false, error: { code, message } };
}
