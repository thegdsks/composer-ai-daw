// Position readout text: bars and beats from the meter map, minutes and seconds from the tempo map.
import { config } from '../config';
import type { TempoMap, Tick } from '../model';
import { barAt, meterSegments, ticksPerBar, ticksPerBeat, ticksToSeconds } from '../time';

export interface PositionText {
  /** Bar, beat and sixteenth of the beat, one based: "5.2.3". */
  musical: string;
  /** m:ss.cc, or h:mm:ss.cc from one hour. */
  time: string;
}

export function formatSeconds(seconds: number): string {
  const total = Math.max(0, seconds);
  const hundredths = Math.floor(total * 100 + 1e-6);
  const cc = hundredths % 100;
  const whole = Math.floor(hundredths / 100);
  const s = whole % 60;
  const m = Math.floor(whole / 60) % 60;
  const h = Math.floor(whole / 3600);
  const pad = (n: number): string => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}.${pad(cc)}` : `${m}:${pad(s)}.${pad(cc)}`;
}

export function formatPosition(map: TempoMap, tick: Tick, tpq = config.ticksPerQuarter): PositionText {
  const t = Math.max(0, Math.round(tick));
  const segs = meterSegments(map, tpq);
  let seg = segs[0]!;
  for (const s of segs) if (s.tick <= t) seg = s;
  const barLen = ticksPerBar(seg.sig, tpq);
  const beatLen = ticksPerBeat(seg.sig, tpq);
  const intoBar = (t - seg.tick) % barLen;
  const beat = Math.floor(intoBar / beatLen) + 1;
  const sixteenth = Math.floor((intoBar % beatLen) / (beatLen / 4)) + 1;
  return {
    musical: `${barAt(map, t, tpq) + 1}.${beat}.${sixteenth}`,
    time: formatSeconds(ticksToSeconds(map, t, tpq)),
  };
}
