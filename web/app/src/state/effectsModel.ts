// Effect chain rules for the UI: the device table (plain names, one line descriptions, parameter
// labels, units and ranges), clamping, value text, and the plans for reorder and drops. Pure, so it
// is tested without React. Ranges and defaults mirror EffectKind::params in core/composer-model;
// tests/effects.test.ts checks them against the engine so a drift fails loudly.
import type { Command, Effect, EffectChain, EffectKind, EffectOwner, Id, Project } from '../model';

export interface ParamUi {
  /** Name sent to set_effect_param. */
  name: string;
  /** Plain label shown next to the control. */
  label: string;
  min: number;
  max: number;
  default: number;
  step: number;
  /** A 0 or 1 switch rather than a slider. */
  toggle?: boolean;
  text(v: number): string;
}

export interface DeviceUi {
  kind: EffectKind;
  name: string;
  blurb: string;
  params: readonly ParamUi[];
}

const percent = (v: number): string => `${Math.round(v / 10)}%`;
const ms = (v: number): string => (v >= 1000 ? `${(v / 1000).toFixed(2)} s` : `${v} ms`);
const onOff = (v: number): string => (v === 0 ? 'Off' : 'On');
const balance = (v: number): string => {
  if (v === 0) return 'Center';
  return `${Math.abs(Math.round(v / 10))}% ${v < 0 ? 'left' : 'right'}`;
};

const p = (
  name: string,
  label: string,
  min: number,
  max: number,
  def: number,
  step: number,
  text: (v: number) => string,
  toggle = false,
): ParamUi => ({ name, label, min, max, default: def, step, text, ...(toggle ? { toggle } : {}) });

export const DEVICES: readonly DeviceUi[] = [
  {
    kind: 'delay',
    name: 'Echo',
    blurb: 'Repeats the sound after a short time, fading away.',
    params: [
      p('time_ms', 'Time', 1, 2000, 250, 1, ms),
      p('feedback', 'Repeats', 0, 950, 350, 10, percent),
      p('filter', 'Softness', 0, 1000, 300, 10, percent),
      p('mix', 'Mix', 0, 1000, 300, 10, percent),
    ],
  },
  {
    kind: 'waveshaper',
    name: 'Distortion',
    blurb: 'Adds warmth and grit by bending the wave.',
    params: [
      p('drive', 'Drive', 0, 1000, 300, 10, percent),
      p('shape', 'Hardness', 0, 1000, 0, 10, percent),
      p('mix', 'Mix', 0, 1000, 1000, 10, percent),
    ],
  },
  {
    kind: 'crusher',
    name: 'Lo-fi crush',
    blurb: 'Makes the sound grainy like an old sampler.',
    params: [
      p('bits', 'Bit depth', 1, 16, 8, 1, (v) => `${v} bits`),
      p('rate_reduction', 'Grain', 1, 64, 1, 1, (v) => (v === 1 ? 'None' : `${v}x coarser`)),
      p('mix', 'Mix', 0, 1000, 1000, 10, percent),
    ],
  },
  {
    kind: 'gate',
    name: 'Gate',
    blurb: 'Silences quiet noise between notes.',
    params: [
      p('threshold', 'Opens at', 0, 1000, 100, 10, percent),
      p('attack_ms', 'Opens in', 0, 200, 1, 1, ms),
      p('release_ms', 'Closes in', 1, 2000, 100, 1, ms),
    ],
  },
  {
    kind: 'tidal',
    name: 'Auto pan and tremolo',
    blurb: 'Swells the volume and sweeps the sound left and right.',
    params: [
      p('rate_centihz', 'Speed', 1, 2000, 200, 1, (v) => `${(v / 100).toFixed(2)} Hz`),
      p('depth', 'Swell', 0, 1000, 500, 10, percent),
      p('shape', 'Shape', 0, 1000, 0, 10, percent),
      p('pan_depth', 'Sweep', 0, 1000, 0, 10, percent),
    ],
  },
  {
    kind: 'stereo_tool',
    name: 'Balance and width',
    blurb: 'Sets level and balance, flips or sums to mono.',
    params: [
      p('volume', 'Level', 0, 2000, 1000, 10, percent),
      p('pan', 'Balance', -1000, 1000, 0, 10, balance),
      p('invert_left', 'Flip left', 0, 1, 0, 1, onOff, true),
      p('invert_right', 'Flip right', 0, 1, 0, 1, onOff, true),
      p('mono', 'Mono', 0, 1, 0, 1, onOff, true),
    ],
  },
];

export const deviceOf = (kind: EffectKind): DeviceUi => {
  const d = DEVICES.find((x) => x.kind === kind);
  if (!d) throw new Error(`unknown effect kind ${kind}`);
  return d;
};

export const paramOf = (kind: EffectKind, name: string): ParamUi | undefined => deviceOf(kind).params.find((x) => x.name === name);

/** A value held inside the range of its parameter, rounded to a whole number. Unknown names pass through rounded. */
export function clampParam(kind: EffectKind, name: string, v: number): number {
  const spec = paramOf(kind, name);
  const n = Math.round(Number.isFinite(v) ? v : 0);
  return spec ? Math.min(spec.max, Math.max(spec.min, n)) : n;
}

export function paramText(kind: EffectKind, name: string, v: number): string {
  const spec = paramOf(kind, name);
  return spec ? spec.text(clampParam(kind, name, v)) : String(v);
}

export const ownerKey = (o: EffectOwner): string => (o.kind === 'master' ? 'master' : `track:${o.track}`);

/** The devices of one owner in processing order, or an empty list. */
export function chainOf(project: Project, owner: EffectOwner): readonly Effect[] {
  const key = ownerKey(owner);
  return project.effects?.find((c: EffectChain) => ownerKey(c.owner) === key)?.effects ?? [];
}

/** Value of a parameter by name, falling back to the default when the engine sent fewer values. */
export function paramValue(e: Effect): (name: string) => number {
  const specs = deviceOf(e.kind).params;
  return (name) => {
    const i = specs.findIndex((s) => s.name === name);
    return e.params[i] ?? specs[i]?.default ?? 0;
  };
}

export function addEffectCommand(id: Id, owner: EffectOwner, kind: EffectKind, index: number | null = null): Command {
  return { command: 'add_effect', id, owner, kind, index };
}

/** Target index for moving one step up (delta -1) or down (+1); null at the ends so the button can say why. */
export function stepIndex(length: number, from: number, delta: -1 | 1): number | null {
  const to = from + delta;
  return from < 0 || from >= length || to < 0 || to >= length ? null : to;
}

export function moveEffectCommand(effect: Id, index: number): Command {
  return { command: 'move_effect', effect, index };
}

/** Index a card dropped on another card takes: where the target card is now. Null when it would not move. */
export function dropIndex(chain: readonly Effect[], dragged: Id, onto: Id): number | null {
  const from = chain.findIndex((e) => e.id === dragged);
  const to = chain.findIndex((e) => e.id === onto);
  return from < 0 || to < 0 || from === to ? null : to;
}

/** The command for one parameter edit, or null when the clamped value is already set. */
export function paramCommand(effect: Effect, name: string, value: number): Command | null {
  const v = clampParam(effect.kind, name, value);
  return v === paramValue(effect)(name) ? null : { command: 'set_effect_param', effect: effect.id, param: name, value: v };
}

/** Why an owner cannot take effects: the master and instrument tracks do, audio tracks do not yet (D45). */
export function ownerRefusal(project: Project, owner: EffectOwner): string | null {
  if (owner.kind === 'master') return null;
  const t = project.tracks.find((x) => x.id === owner.track);
  if (!t) return 'That track is gone.';
  return t.kind === 'audio' ? 'Effects are not available on audio tracks yet.' : null;
}
