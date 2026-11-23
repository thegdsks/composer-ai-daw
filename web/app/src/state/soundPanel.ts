// Pure rules for the sound panel and the master strip: ranges, plain labels, neighbour sound.
import { config } from '../config';
import type { SoundEntry } from '../backend/types';
import { INSTRUMENT_SAW, INSTRUMENT_SINE, type Id, type MasterMix, type Project, type Track } from '../model';
import { availability, categoryOf, type CategoryLabel } from '../sounds/catalogue';

export function clampReverb(v: number): number {
  return Math.min(config.reverbMax, Math.max(0, Math.round(v)));
}

export function reverbSendOf(t: Track): number {
  return clampReverb(t.reverb_send ?? 0);
}

export function masterOf(p: Project): MasterMix {
  return {
    volume: p.master?.volume ?? config.volumeUnity,
    reverb_size: p.master?.reverb_size ?? config.reverbSizeDefault,
    reverb_mix: p.master?.reverb_mix ?? 0,
  };
}

/** Reverb amounts read as percent of the full range; zero says Off. */
export function reverbText(permille: number): string {
  if (permille <= 0) return 'Off';
  return `${Math.round((permille / config.reverbMax) * 100)} percent`;
}

export function trackSendText(permille: number): string {
  return permille <= 0 ? 'None' : reverbText(permille);
}

/** Name of what a track plays: the catalogue sound, else the basic instrument. */
export function soundLabel(track: Track, entries: readonly SoundEntry[]): string {
  if (track.sound) return entries.find((e) => e.id === track.sound)?.name ?? 'Custom sound';
  return 'Basic instrument';
}

export function instrumentLabel(track: Track): string {
  if (track.sound) return 'Catalogue sound';
  if (track.instrument === INSTRUMENT_SINE) return 'Sine';
  if (track.instrument === INSTRUMENT_SAW) return 'Saw';
  return track.instrument === null ? 'No instrument' : 'Custom instrument';
}

export function categoryName(track: Track, entries: readonly SoundEntry[]): CategoryLabel | null {
  const e = entries.find((x) => x.id === track.sound);
  return e ? categoryOf(e) : null;
}

/**
 * Next or previous playable sound, wrapping at the ends. It steps inside the current sound's
 * category; with no current sound it steps through every playable sound. Hidden and sample
 * pack entries are skipped. Returns null when there is nothing to step to.
 */
export function neighbourSound(entries: readonly SoundEntry[], current: Id | null, dir: 1 | -1): SoundEntry | null {
  const ready = entries.filter((e) => availability(e).state === 'ready');
  const now = ready.find((e) => e.id === current);
  const pool = now ? ready.filter((e) => categoryOf(e) === categoryOf(now)) : ready;
  if (pool.length === 0 || (pool.length === 1 && now)) return null;
  const at = now ? pool.indexOf(now) : dir === 1 ? -1 : 0;
  return pool[(at + dir + pool.length) % pool.length] ?? null;
}
