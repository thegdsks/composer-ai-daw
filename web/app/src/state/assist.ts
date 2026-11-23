// AI assist in the palette, the logic half (no React). There is no model inside the app: cards
// come from proposals that AI clients posted and from local recipes planned against the project.
// Everything here is a pure function, so the plain-language summary and the card rules are testable.
import type { Command } from '../model';
import type { Proposal, RecipeInfo, RecipePlan, RecipeRequest, RecipeScope } from '../backend/types';

export type CardOrigin = 'proposal' | 'recipe';

export interface AssistCard {
  /** Stable key: `proposal:<id>` or `recipe:<name>`. */
  key: string;
  origin: CardOrigin;
  /** Short neutral tag shown on the card. */
  tag: string;
  label: string;
  rationale: string;
  author: string | null;
  commandCount: number;
  changeCount: number;
  /** Plain sentences about what would change. */
  changes: string[];
  commands: Command[];
  /** The project moved since a proposal was posted. */
  stale: boolean;
  /** Whether the commands still apply to the project as it is now. */
  applies: boolean;
  proposalId?: string;
  request?: RecipeRequest;
}

/** What the palette knows about the selection when it opens. */
export interface AssistContext {
  track: string | null;
  region: string | null;
  tracks: number;
}

type Bucket =
  | 'tracks+'
  | 'tracks-'
  | 'clips+'
  | 'clips-'
  | 'clips~'
  | 'notes+'
  | 'notes-'
  | 'notes~'
  | 'loudness'
  | 'tempo'
  | 'key'
  | 'meter'
  | 'markers'
  | 'mix'
  | 'sound'
  | 'other';

const ORDER: Bucket[] = ['tracks+', 'tracks-', 'clips+', 'clips-', 'clips~', 'notes+', 'notes-', 'notes~', 'loudness', 'tempo', 'key', 'meter', 'markers', 'mix', 'sound', 'other'];

function count(value: unknown, fallback: number): number {
  return Array.isArray(value) ? value.length : fallback;
}

/** Which kind of edit a command is and how many things it touches. */
function classify(c: Command): [Bucket, number] {
  const f = c as unknown as Record<string, unknown>;
  switch (c.command) {
    case 'add_track':
      return ['tracks+', 1];
    case 'delete_track':
      return ['tracks-', 1];
    case 'add_region':
      return ['clips+', 1];
    case 'delete_region':
      return ['clips-', 1];
    case 'move_region':
    case 'resize_region':
    case 'set_region_loop':
    case 'move_region_to_track':
      return ['clips~', 1];
    case 'add_note':
      return ['notes+', 1];
    case 'add_notes':
      return ['notes+', count(f.notes, 1)];
    case 'delete_note':
      return ['notes-', 1];
    case 'set_note_velocity':
      return ['loudness', 1];
    case 'set_notes_velocity':
      return ['loudness', count(f.notes, 1)];
    case 'move_note':
    case 'resize_note':
      return ['notes~', 1];
    case 'move_notes':
    case 'resize_notes':
    case 'transpose_notes':
    case 'quantize_notes':
      return ['notes~', count(f.notes, 1)];
    case 'set_tempo':
    case 'remove_tempo':
      return ['tempo', 1];
    case 'set_key':
    case 'remove_key':
      return ['key', 1];
    case 'set_time_signature':
    case 'remove_time_signature':
      return ['meter', 1];
    case 'set_marker':
    case 'remove_marker':
      return ['markers', 1];
    case 'set_track_volume':
    case 'set_track_pan':
    case 'set_track_reverb_send':
    case 'set_master_volume':
    case 'set_master_reverb':
    case 'set_track_mute':
    case 'set_track_solo':
      return ['mix', 1];
    case 'set_instrument':
    case 'set_track_sound':
      return ['sound', 1];
    default:
      return ['other', 1];
  }
}

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

const SENTENCE: Record<Bucket, (n: number) => string> = {
  'tracks+': (n) => `Adds ${plural(n, 'track', 'tracks')}`,
  'tracks-': (n) => `Removes ${plural(n, 'track', 'tracks')}`,
  'clips+': (n) => `Adds ${plural(n, 'clip', 'clips')}`,
  'clips-': (n) => `Removes ${plural(n, 'clip', 'clips')}`,
  'clips~': (n) => `Moves or resizes ${plural(n, 'clip', 'clips')}`,
  'notes+': (n) => `Adds ${plural(n, 'note', 'notes')}`,
  'notes-': (n) => `Removes ${plural(n, 'note', 'notes')}`,
  'notes~': (n) => `Changes ${plural(n, 'note', 'notes')}`,
  loudness: (n) => `Changes how loud ${plural(n, 'note is', 'notes are')}`,
  tempo: (n) => `Changes the tempo (${plural(n, 'change', 'changes')})`,
  key: (n) => `Changes the key (${plural(n, 'change', 'changes')})`,
  meter: (n) => `Changes the time signature (${plural(n, 'change', 'changes')})`,
  markers: (n) => `Changes ${plural(n, 'marker', 'markers')}`,
  mix: (n) => `Changes the mix (${plural(n, 'setting', 'settings')})`,
  sound: (n) => `Changes the sound of ${plural(n, 'track', 'tracks')}`,
  other: (n) => `Makes ${plural(n, 'other edit', 'other edits')}`,
};

/** Plain sentences for what a command list does, in a fixed order, one per kind of edit. */
export function describeChanges(commands: readonly Command[]): string[] {
  const totals = new Map<Bucket, number>();
  for (const c of commands) {
    const [bucket, n] = classify(c);
    totals.set(bucket, (totals.get(bucket) ?? 0) + n);
  }
  return ORDER.filter((b) => totals.has(b)).map((b) => `${SENTENCE[b](totals.get(b) ?? 0)}.`);
}

/** One line per command for the disclosed list, shortened so a long note list stays readable. */
export function commandLine(c: Command, max = 110): string {
  const text = JSON.stringify(c);
  return text.length > max ? `${text.slice(0, max - 1)}...` : text;
}

/** Core recipe texts say velocity; the app says loudness. */
function plainWords(text: string): string {
  return text.replace(/velocity/gi, (m) => (m[0] === 'V' ? 'Loudness' : 'loudness'));
}

export function cardFromProposal(p: Proposal): AssistCard {
  return {
    key: `proposal:${p.id}`,
    origin: 'proposal',
    tag: 'AI',
    label: p.label,
    rationale: p.note?.trim() || 'An AI client suggested this change. Nothing happens until you accept.',
    author: p.author,
    commandCount: p.commandCount,
    changeCount: p.changeCount ?? 0,
    changes: describeChanges(p.commands),
    commands: p.commands,
    stale: p.stale,
    applies: p.applies ?? true,
    proposalId: p.id,
  };
}

export function cardFromRecipe(request: RecipeRequest, info: RecipeInfo, plan: RecipePlan): AssistCard {
  return {
    key: `recipe:${request.recipe}`,
    origin: 'recipe',
    tag: 'Recipe',
    label: plainWords(plan.label || info.title),
    rationale: plainWords(info.summary),
    author: null,
    commandCount: plan.commandCount,
    changeCount: plan.changeCount,
    changes: describeChanges(plan.commands),
    commands: plan.commands,
    stale: false,
    applies: plan.applies,
    request,
  };
}

/** Recipes worth planning for this selection, most useful first. A failed plan simply hides the card. */
export function candidateRequests(ctx: AssistContext, recipes: readonly RecipeInfo[]): RecipeRequest[] {
  const have = new Set(recipes.map((r) => r.name));
  const scope: RecipeScope = {
    ...(ctx.track ? { track: ctx.track } : {}),
    ...(ctx.region ? { region: ctx.region } : {}),
  };
  const wanted: string[] = ctx.tracks === 0
    ? ['calm_sections']
    : ['bass_from_chords', 'pad_under_melody', 'extend_to_length', 'repeat_with_variation', 'soften_velocity'];
  return wanted.filter((n) => have.has(n)).map((recipe) => ({ recipe, scope: recipe === 'calm_sections' ? {} : scope }));
}

/** Plain sentence for a card that cannot run now. */
export function blockedReason(card: AssistCard): string | null {
  if (!card.applies) return 'This no longer fits the project, because something it needs has changed. Reject it to clear it.';
  return null;
}

/** Note shown on a card whose project moved on but which still fits. */
export function staleNote(card: AssistCard): string | null {
  return card.stale && card.applies ? 'The project changed after this was posted. It still fits, so you can preview it.' : null;
}
