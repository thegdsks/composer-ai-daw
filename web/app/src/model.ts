// Types for the project JSON and the command API (core/composer-commands, decision D9).
// Ticks are integers at 960 per quarter note; ids are 32 hex digits.

import { config } from './config';

export type Id = string;
export type Tick = number;

export interface Note {
  id: Id;
  pitch: number;
  velocity: number;
  start: Tick;
  duration: Tick;
}

export interface MidiContent {
  midi: { notes: Record<Id, Note> };
}

export interface Region {
  id: Id;
  start: Tick;
  length: Tick;
  /** Ticks of a midi clip that repeat to fill it (D24); absent while there is no loop. */
  loop_length?: Tick;
  content: MidiContent | { audio: unknown };
}

export type TrackKind = 'instrument' | 'audio';

/** Mixer state in permille; the daemon leaves the field out while it is all defaults. */
export interface Mix {
  volume: number;
  pan: number;
  mute: boolean;
  solo: boolean;
}

export interface Track {
  id: Id;
  name: string;
  kind: TrackKind;
  instrument: Id | null;
  /** Catalogue sound id (D29); when set, the engine plays it instead of `instrument`. */
  sound?: string;
  /** Share of the track sent to the master reverb in permille; absent while 0. */
  reverb_send?: number;
  regions: Record<Id, Region>;
  mix?: Partial<Mix>;
}

export interface TimeSignature {
  numerator: number;
  denominator: number;
}

export interface TempoMap {
  /** Keyed by tick (as text), value is thousandths of a BPM. */
  tempos: Record<string, number>;
  meters: Record<string, TimeSignature>;
  /** Key changes by tick (as text); absent when the project has no key (D23). */
  keys?: Record<string, { root: number; scale: ScaleName }>;
}

export type ScaleName =
  | 'major'
  | 'minor'
  | 'dorian'
  | 'mixolydian'
  | 'pentatonic_major'
  | 'pentatonic_minor'
  | 'harmonic_minor';

export interface Project {
  id: Id;
  name: string;
  tempo_map: TempoMap;
  tracks: Track[];
  /** Master volume and reverb; absent while all are default. */
  master?: Partial<MasterMix>;
  /** Named positions by tick (as text), D24; absent while there are none. */
  markers?: Record<string, string>;
  /** Effect chains, one per owner; absent while there is none (D45). */
  effects?: EffectChain[];
}

export type EffectKind = 'stereo_tool' | 'delay' | 'crusher' | 'gate' | 'waveshaper' | 'tidal';

export type EffectOwner = { kind: 'master' } | { kind: 'track'; track: Id };

/** One device of a chain (D45). `params` follows the order of the kind's parameter table. */
export interface Effect {
  id: Id;
  kind: EffectKind;
  /** Absent while false. */
  bypass?: boolean;
  params: number[];
}

export interface EffectChain {
  owner: EffectOwner;
  effects: Effect[];
}

export interface MasterMix {
  volume: number;
  reverb_size: number;
  reverb_mix: number;
}

export type Command =
  | { command: 'set_project_name'; name: string }
  | { command: 'set_tempo'; tick: Tick; mbpm: number }
  | { command: 'remove_tempo'; tick: Tick }
  | { command: 'set_time_signature'; tick: Tick; numerator: number; denominator: number }
  | { command: 'remove_time_signature'; tick: Tick }
  | { command: 'set_key'; tick: Tick; root: number; scale: ScaleName }
  | { command: 'remove_key'; tick: Tick }
  | { command: 'set_marker'; tick: Tick; name: string }
  | { command: 'remove_marker'; tick: Tick }
  | { command: 'set_region_loop'; region: Id; loop_length: Tick }
  | { command: 'add_track'; id: Id; name: string; kind: TrackKind; index: number | null }
  | { command: 'rename_track'; track: Id; name: string }
  | { command: 'set_instrument'; track: Id; instrument: Id | null }
  | { command: 'set_track_sound'; track: Id; sound: string | null }
  | { command: 'move_track'; track: Id; index: number }
  | { command: 'delete_track'; track: Id }
  | {
      command: 'add_region';
      id: Id;
      track: Id;
      start: Tick;
      length: Tick;
      kind: 'midi' | 'audio';
    }
  | { command: 'move_region'; region: Id; start: Tick }
  | { command: 'resize_region'; region: Id; length: Tick }
  | { command: 'delete_region'; region: Id }
  | { command: 'set_track_volume'; track: Id; volume: number }
  | { command: 'set_track_pan'; track: Id; pan: number }
  | { command: 'set_track_reverb_send'; track: Id; send: number }
  | { command: 'set_master_volume'; volume: number }
  | { command: 'set_master_reverb'; size: number; mix: number }
  | { command: 'add_effect'; id: Id; owner: EffectOwner; kind: EffectKind; index: number | null }
  | { command: 'remove_effect'; effect: Id }
  | { command: 'move_effect'; effect: Id; index: number }
  | { command: 'set_effect_bypass'; effect: Id; bypass: boolean }
  | { command: 'set_effect_param'; effect: Id; param: string; value: number }
  | { command: 'set_track_mute'; track: Id; mute: boolean }
  | { command: 'set_track_solo'; track: Id; solo: boolean }
  | { command: 'move_region_to_track'; region: Id; track: Id }
  | {
      command: 'add_note';
      id: Id;
      region: Id;
      pitch: number;
      velocity: number;
      start: Tick;
      duration: Tick;
    }
  | { command: 'move_note'; region: Id; note: Id; start: Tick; pitch: number }
  | { command: 'resize_note'; region: Id; note: Id; duration: Tick }
  | { command: 'set_note_velocity'; region: Id; note: Id; velocity: number }
  | { command: 'delete_note'; region: Id; note: Id }
  | { command: 'add_notes'; region: Id; notes: { id: Id; pitch: number; velocity: number; start: Tick; duration: Tick }[] }
  | { command: 'transpose_notes'; region: Id; semitones: number; notes: Id[] | null }
  | { command: 'quantize_notes'; region: Id; grid: Tick; strength: number; notes: Id[] | null }
  | { command: 'move_notes'; region: Id; notes: Id[]; delta_tick: Tick; delta_pitch: number }
  | { command: 'resize_notes'; region: Id; notes: Id[]; delta: Tick }
  | {
      command: 'set_notes_velocity';
      region: Id;
      notes: Id[];
      value: number | null;
      ramp: { from: number; to: number } | null;
    }
  | { command: 'delete_notes'; region: Id; notes: Id[] }
  | { command: 'split_region'; region: Id; at: Tick; new_region: Id; note_ids: Id[] }
  | { command: 'duplicate_region'; region: Id; new_region: Id; start: Tick; track: Id | null; note_ids: Id[] };

export interface CommandInfo {
  name: string;
  summary: string;
  fields: { name: string; ty: string; nullable: boolean }[];
  /** Palette metadata (D33): add, change, remove, go_to or suggest, plain label and search words. */
  verb?: string;
  label?: string;
  keywords?: string[];
}

/** Built-in instrument ids understood by the engine (composer-engine BUILTIN_*). */
export const INSTRUMENT_SINE: Id = '00000000000000000000000000000001';
export const INSTRUMENT_SAW: Id = '00000000000000000000000000000002';

/** Mixer state with the daemon's defaults filled in. */
export function mixOf(t: Track): Mix {
  return {
    volume: t.mix?.volume ?? config.volumeUnity,
    pan: t.mix?.pan ?? 0,
    mute: t.mix?.mute ?? false,
    solo: t.mix?.solo ?? false,
  };
}

export function isMidi(r: Region): r is Region & { content: MidiContent } {
  return 'midi' in r.content;
}

export function notesOf(r: Region): Note[] {
  return isMidi(r) ? Object.values(r.content.midi.notes) : [];
}
