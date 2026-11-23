// Turns tempo map and transport prefs into the frame based plan the worklet follows.
import type { TransportPrefs } from '../backend/types';
import { config } from '../config';
import type { TempoMap } from '../model';
import { beatClicks, ticksToSeconds } from '../time';
import type { TransportPlan } from './protocol';

export interface TransportInputs {
  map: TempoMap;
  prefs: TransportPrefs;
  /** Last tick worth clicking: the song end or the loop end, whichever is later. */
  endTick: number;
}

/** Clicks are only computed while the metronome is on, so an idle session pays nothing. */
export function buildPlan(i: TransportInputs, sampleRate: number): TransportPlan {
  const { loop, metronome } = i.prefs;
  const frame = (tick: number): number => Math.round(ticksToSeconds(i.map, tick) * sampleRate);
  const horizon = Math.max(i.endTick, loop.on ? loop.end : 0) + config.ticksPerQuarter * 4;
  const clicks = metronome.on ? beatClicks(i.map, 0, horizon, sampleRate) : [];
  return {
    loop: { on: loop.on && loop.end > loop.start, startFrame: frame(loop.start), endFrame: frame(loop.end) },
    metronome: metronome.on,
    beats: Float64Array.from(clicks, (c) => c.frame),
    accents: Uint8Array.from(clicks, (c) => (c.accent ? 1 : 0)),
  };
}

/** Stable text of the inputs, to skip a rebuild when nothing a plan depends on changed. */
export function planKey(i: TransportInputs): string {
  const { loop, metronome } = i.prefs;
  return `${JSON.stringify(i.map)}|${loop.on}|${loop.start}|${loop.end}|${metronome.on}|${i.endTick}`;
}
