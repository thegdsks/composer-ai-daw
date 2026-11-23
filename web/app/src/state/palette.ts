// Command palette logic: entries from describe() metadata (D33) plus a few app actions, word
// start matching, local usage ranking, and a small form plan built from field types. Pure
// functions, no React, so a test can check every described command has a plain entry.
import { config } from '../config';
import type { Command, CommandInfo, Id, Project } from '../model';
import { bpmAt } from '../time';
import type { CommandEnabled } from '../backend/types';

/** What the palette needs to know about the current selection. */
export interface PaletteContext {
  project: Project;
  track: Id | null;
  region: Id | null;
  notes: readonly Id[];
}

export interface PaletteItem {
  /** Command name for described commands, `app:<name>` for app actions. */
  id: string;
  label: string;
  verb: string;
  keywords: string[];
  enabled: boolean;
  /** Plain sentence shown when the entry cannot run now. */
  reason: string | null;
  info: CommandInfo | null;
  /** Shortcut text for app actions that have one. */
  keys?: string;
}

export interface AppItem {
  name: string;
  label: string;
  verb: string;
  keywords: string[];
  keys?: string;
}

export const APP_ITEMS: readonly AppItem[] = [
  { name: 'play', label: 'Play or pause', verb: 'go_to', keywords: ['start', 'stop', 'listen', 'hear'], keys: 'Space' },
  { name: 'undo', label: 'Undo the last step', verb: 'go_to', keywords: ['back', 'revert', 'oops'], keys: 'Ctrl or Cmd Z' },
  { name: 'redo', label: 'Redo the step you undid', verb: 'go_to', keywords: ['forward', 'again'], keys: 'Ctrl or Cmd Shift Z' },
  { name: 'copy', label: 'Copy the selected notes or clips', verb: 'change', keywords: ['clipboard', 'duplicate'], keys: 'Ctrl or Cmd C' },
  { name: 'cut', label: 'Cut the selected notes or clips', verb: 'remove', keywords: ['clipboard', 'move'], keys: 'Ctrl or Cmd X' },
  { name: 'paste', label: 'Paste at the playhead bar', verb: 'add', keywords: ['clipboard', 'insert'], keys: 'Ctrl or Cmd V' },
  { name: 'duplicate', label: 'Duplicate right after itself', verb: 'add', keywords: ['copy', 'twin', 'again'], keys: 'Ctrl or Cmd D' },
  { name: 'repeat', label: 'Repeat a number of times', verb: 'add', keywords: ['loop', 'copies', 'times', 'again'], keys: 'Ctrl or Cmd Shift D' },
  { name: 'quantize', label: 'Tidy the timing of the selected notes', verb: 'change', keywords: ['quantize', 'snap', 'grid', 'tighten', 'timing', 'grid'], keys: 'Q' },
  { name: 'loop', label: 'Loop on or off', verb: 'change', keywords: ['repeat', 'cycle'], keys: 'L' },
  { name: 'metronome', label: 'Metronome on or off', verb: 'change', keywords: ['click', 'tempo', 'beat'], keys: 'M' },
  { name: 'history', label: 'Show history', verb: 'go_to', keywords: ['steps', 'undo list', 'timeline', 'back in time'], keys: 'Ctrl or Cmd Alt Z' },
  { name: 'help', label: 'Show keys and help', verb: 'go_to', keywords: ['shortcuts', 'keyboard', 'how'], keys: 'Shift /' },
  { name: 'start', label: 'Show the start screen', verb: 'go_to', keywords: ['template', 'new', 'begin', 'goal'] },
];

/** Commands the palette hands to a better place, with the plain sentence it shows. */
const BLOCKED: Record<string, string> = {
  set_instrument: 'Pick a sound in the Sound tab.',
  set_track_sound: 'Pick a sound in the Sound tab.',
  set_track_volume: 'Use the Mix tab for volume.',
  set_track_pan: 'Use the Mix tab for pan.',
  set_track_reverb_send: 'Use the Sound tab for reverb send.',
  set_master_volume: 'Use the master strip in the Mix tab.',
  set_master_reverb: 'Use the master strip in the Mix tab.',
  set_track_mute: 'Use the Mix tab for mute.',
  set_track_solo: 'Use the Mix tab for solo.',
  move_region_to_track: 'Drag the clip onto another track.',
  split_region: 'Press S in the arrangement to split at the playhead.',
  duplicate_region: 'Copy and paste the clip in the arrangement.',
  add_notes: 'Add notes in the piano roll.',
  set_notes_velocity: 'Drag the loudness bars in the piano roll.',
  move_notes: 'Drag or use the arrow keys in the piano roll.',
  resize_notes: 'Drag a note edge or use Shift and arrows in the piano roll.',
};

export function words(text: string): string[] {
  return text.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
}

/** Plain name of a described command; a missing label falls back to its id in words. */
export function plainName(info: CommandInfo): string {
  if (info.label?.trim()) return info.label;
  const w = info.name.replace(/_/g, ' ');
  return w.charAt(0).toUpperCase() + w.slice(1);
}

/** Why the current selection cannot feed this command, or null. */
export function selectionReason(info: CommandInfo, ctx: PaletteContext): string | null {
  const blocked = BLOCKED[info.name];
  if (blocked) return blocked;
  for (const f of info.fields) {
    if (f.name === 'track' && f.ty === 'id' && !ctx.track) return 'Select a track first.';
    if (f.name === 'region' && !ctx.region) return 'Select a clip first.';
    if (f.name === 'note' && ctx.notes.length !== 1) return 'Select one note first.';
    if (f.name === 'notes' && f.ty === 'ids' && ctx.notes.length === 0) return 'Select some notes first.';
  }
  return null;
}

export function buildItems(
  infos: readonly CommandInfo[],
  enabled: readonly CommandEnabled[],
  ctx: PaletteContext,
): PaletteItem[] {
  const by = new Map(enabled.map((e) => [e.name, e]));
  const items: PaletteItem[] = [];
  for (const a of APP_ITEMS) {
    items.push({ id: `app:${a.name}`, label: a.label, verb: a.verb, keywords: a.keywords, enabled: true, reason: null, info: null, keys: a.keys });
  }
  for (const info of infos) {
    const state = by.get(info.name);
    const reason = state && !state.enabled ? (state.reason ?? 'Not available right now.') : selectionReason(info, ctx);
    items.push({
      id: info.name,
      label: plainName(info),
      verb: info.verb ?? 'change',
      keywords: info.keywords ?? [],
      enabled: reason === null,
      reason,
      info,
    });
  }
  return items;
}

export type Usage = Record<string, number>;

/** Higher is better; negative means no match. Every query word must start a word of the entry. */
export function scoreItem(item: PaletteItem, query: string, usage: Usage): number {
  const q = words(query);
  const used = Math.min(usage[item.id] ?? 0, 20);
  if (q.length === 0) return used;
  const labelWords = words(item.label);
  const rest = words(`${item.verb} ${item.keywords.join(' ')}`);
  let score = 0;
  for (const w of q) {
    const inLabel = labelWords.findIndex((x) => x.startsWith(w));
    if (inLabel >= 0) score += inLabel === 0 ? 30 : 20;
    else if (rest.some((x) => x.startsWith(w))) score += 8;
    else return -1;
  }
  return score + used;
}

export function matchItems(items: readonly PaletteItem[], query: string, usage: Usage, max = config.paletteMaxResults): PaletteItem[] {
  return items
    .map((item, i) => ({ item, i, score: scoreItem(item, query, usage) }))
    .filter((x) => x.score >= 0)
    .sort((a, b) => b.score - a.score || Number(b.item.enabled) - Number(a.item.enabled) || a.i - b.i)
    .slice(0, max)
    .map((x) => x.item);
}

// Usage ranking is local only: a count per entry, trimmed to a configured size.
const USAGE_KEY = 'composer.palette.usage.v1';

export function loadUsage(): Usage {
  try {
    const raw = localStorage.getItem(USAGE_KEY);
    const v: unknown = raw ? JSON.parse(raw) : {};
    if (!v || typeof v !== 'object') return {};
    const out: Usage = {};
    for (const [k, n] of Object.entries(v)) if (typeof n === 'number' && Number.isFinite(n)) out[k] = n;
    return out;
  } catch {
    return {};
  }
}

export function bumpUsage(usage: Usage, id: string): Usage {
  const next: Usage = { ...usage, [id]: (usage[id] ?? 0) + 1 };
  const keys = Object.keys(next);
  if (keys.length > config.paletteUsageMax) {
    keys.sort((a, b) => (next[a] ?? 0) - (next[b] ?? 0));
    for (const k of keys.slice(0, keys.length - config.paletteUsageMax)) delete next[k];
  }
  try {
    localStorage.setItem(USAGE_KEY, JSON.stringify(next));
  } catch {
    // ranking is a nicety; a full or blocked store just forgets it
  }
  return next;
}

export interface FormField {
  name: string;
  label: string;
  kind: 'text' | 'number' | 'select' | 'checkbox';
  options?: { value: string; label: string }[];
  initial: string | boolean;
  min?: number;
  max?: number;
  step?: number;
  /** Blank is allowed and sends null. */
  optional: boolean;
}

export interface FormPlan {
  /** Fields the person fills in. Empty means the command can run straight away. */
  ask: FormField[];
  /** Values taken from the selection or generated. */
  auto: Record<string, unknown>;
}

const NOTE_NAMES = ['C', 'C sharp', 'D', 'D sharp', 'E', 'F', 'F sharp', 'G', 'G sharp', 'A', 'A sharp', 'B'];
const BEAT_POSITION = new Set(['tick', 'start', 'at']);
const BEAT_LENGTH = new Set(['length', 'duration', 'grid', 'delta', 'delta_tick']);

function sentence(text: string): string {
  const w = text.replace(/_/g, ' ');
  return w.charAt(0).toUpperCase() + w.slice(1);
}

function initialFor(info: CommandInfo, name: string, ctx: PaletteContext): string {
  const track = ctx.project.tracks.find((t) => t.id === ctx.track);
  if (info.name === 'set_project_name') return ctx.project.name;
  if (info.name === 'rename_track') return track?.name ?? '';
  if (name === 'mbpm') return String(Math.round(bpmAt(ctx.project.tempo_map, 0) * 100) / 100);
  if (name === 'numerator') return '4';
  if (name === 'denominator') return '4';
  if (name === 'velocity') return String(config.defaultNoteVelocity);
  if (name === 'pitch') return '60';
  if (name === 'strength') return '100';
  if (name === 'semitones') return '12';
  if (name === 'tick' || name === 'start' || name === 'at') return '1';
  if (name === 'length' || name === 'duration' || name === 'grid') return name === 'grid' ? '0.5' : '4';
  return '';
}

function askField(info: CommandInfo, f: CommandInfo['fields'][number], ctx: PaletteContext): FormField {
  const base = { name: f.name, optional: f.nullable, initial: initialFor(info, f.name, ctx) };
  if (f.ty.includes('|')) {
    const isScale = f.name === 'scale';
    return {
      ...base,
      kind: 'select',
      label: isScale ? 'Scale' : sentence(f.name === 'kind' ? 'Kind' : f.name),
      options: f.ty.split('|').map((v) => ({ value: v, label: sentence(v) })),
      initial: f.ty.split('|')[0] ?? '',
    };
  }
  if (f.name === 'root') {
    return { ...base, kind: 'select', label: 'Key note', options: NOTE_NAMES.map((n, i) => ({ value: String(i), label: n })), initial: '0' };
  }
  if (f.ty === 'bool') return { ...base, kind: 'checkbox', label: sentence(f.name), initial: true };
  if (f.ty === 'text') return { ...base, kind: 'text', label: sentence(f.name === 'name' ? 'name' : f.name) };
  const num = (label: string, extra: Partial<FormField> = {}): FormField => ({ ...base, kind: 'number', label, ...extra });
  if (f.name === 'mbpm') return num('Tempo (beats per minute)', { min: 1, step: 1 });
  if (BEAT_POSITION.has(f.name)) return num('At beat (1 is the start)', { min: 1, step: 0.25 });
  if (BEAT_LENGTH.has(f.name)) return num(f.name === 'grid' ? 'Grid in beats (0.5 is an eighth note)' : 'Length in beats', { step: 0.25 });
  if (f.name === 'index') return num('Place in the list (blank for the end)', { min: 1, step: 1 });
  if (f.name === 'pitch') return num('Pitch (0 to 127, 60 is middle C)', { min: 0, max: 127, step: 1 });
  if (f.name === 'velocity' || f.name === 'value') return num('Loudness (1 to 127)', { min: 1, max: 127, step: 1 });
  if (f.name === 'strength') return num('How much to tidy (0 to 100)', { min: 0, max: 100, step: 1 });
  if (f.name === 'semitones') return num('Steps up (negative goes down)', { step: 1 });
  if (f.name === 'delta_pitch') return num('Steps up (negative goes down)', { step: 1 });
  if (f.name === 'numerator') return num('Beats per bar', { min: 1, step: 1 });
  if (f.name === 'denominator') return num('Note value of a beat (4 is a quarter note)', { min: 1, step: 1 });
  return num(sentence(f.name), { step: 1 });
}

/** Splits a command's fields into what the selection supplies and what the person is asked. */
export function planForm(info: CommandInfo, ctx: PaletteContext, newId: () => Id): FormPlan {
  const ask: FormField[] = [];
  const auto: Record<string, unknown> = {};
  for (const f of info.fields) {
    if (f.ty === 'id') {
      if (f.name === 'id' || f.name === 'new_region') auto[f.name] = newId();
      else if (f.name === 'track') auto.track = ctx.track;
      else if (f.name === 'region') auto.region = ctx.region;
      else if (f.name === 'note') auto.note = ctx.notes[0];
      else auto[f.name] = null;
    } else if (f.ty === 'ids') {
      auto[f.name] = f.name === 'notes' ? [...ctx.notes] : [];
    } else if (f.ty === 'object' || f.ty === 'notes') {
      auto[f.name] = null;
    } else {
      ask.push(askField(info, f, ctx));
    }
  }
  return { ask, auto };
}

export type BuildResult = { ok: true; command: Command } | { ok: false; message: string };

/** Turns form values into a command. Plain words for every failure. */
export function buildCommand(info: CommandInfo, plan: FormPlan, values: Record<string, string | boolean>): BuildResult {
  const out: Record<string, unknown> = { command: info.name, ...plan.auto };
  for (const f of plan.ask) {
    const raw = values[f.name] ?? f.initial;
    if (f.kind === 'checkbox') {
      out[f.name] = raw === true;
      continue;
    }
    const text = typeof raw === 'string' ? raw.trim() : '';
    if (text === '') {
      if (!f.optional) return { ok: false, message: `${f.label} needs a value.` };
      out[f.name] = null;
      continue;
    }
    if (f.kind === 'text' || f.kind === 'select') {
      out[f.name] = f.name === 'root' ? Number(text) : text;
      continue;
    }
    const n = Number(text);
    if (!Number.isFinite(n)) return { ok: false, message: `${f.label} must be a number.` };
    if (f.min !== undefined && n < f.min) return { ok: false, message: `${f.label} must be at least ${f.min}.` };
    if (f.max !== undefined && n > f.max) return { ok: false, message: `${f.label} must be at most ${f.max}.` };
    out[f.name] = wireNumber(f.name, n);
  }
  // The command list is data (describe), so the object is checked by the backend, not by a type.
  return { ok: true, command: out as unknown as Command };
}

function wireNumber(name: string, n: number): number {
  if (name === 'mbpm') return Math.round(n * 1000);
  if (BEAT_POSITION.has(name)) return Math.max(0, Math.round((n - 1) * config.ticksPerQuarter));
  if (BEAT_LENGTH.has(name)) return Math.round(n * config.ticksPerQuarter);
  if (name === 'index') return Math.max(0, Math.round(n) - 1);
  return Math.round(n);
}
