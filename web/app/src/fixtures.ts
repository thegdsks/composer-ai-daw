// Fixtures built from commands only. The starter keeps the first screen from being empty
// (T15 designs the real beginner flow); the stress project exercises very long regions.
import { config } from './config';
import { INSTRUMENT_SAW, INSTRUMENT_SINE, type Command } from './model';
import { addNote } from './state/commands';

const STARTER_BPM = 84;
const PAD_VELOCITY = 68;
const BASS_VELOCITY = 82;
// Four chords in C, one bar each: C, Am, F, G.
const CHORDS: readonly (readonly number[])[] = [
  [60, 64, 67],
  [57, 60, 64],
  [53, 57, 60],
  [55, 59, 62],
];
const BASS_ROOTS = [36, 33, 29, 31];

export function starterCommands(nextId: () => string): Command[] {
  const bar = config.ticksPerQuarter * 4;
  const beat = config.ticksPerQuarter;
  const length = bar * CHORDS.length;
  const pad = nextId();
  const bass = nextId();
  const padRegion = nextId();
  const bassRegion = nextId();
  const out: Command[] = [
    { command: 'set_project_name', name: 'Calm in C' },
    { command: 'set_tempo', tick: 0, mbpm: STARTER_BPM * 1000 },
    { command: 'add_track', id: pad, name: 'Pad', kind: 'instrument', index: null },
    { command: 'set_instrument', track: pad, instrument: INSTRUMENT_SINE },
    { command: 'add_track', id: bass, name: 'Bass', kind: 'instrument', index: null },
    { command: 'set_instrument', track: bass, instrument: INSTRUMENT_SAW },
    { command: 'add_region', id: padRegion, track: pad, start: 0, length, kind: 'midi' },
    { command: 'add_region', id: bassRegion, track: bass, start: 0, length, kind: 'midi' },
  ];
  CHORDS.forEach((chord, i) => {
    for (const pitch of chord) {
      out.push(addNote(nextId(), padRegion, pitch, i * bar, bar - beat / 4, PAD_VELOCITY));
    }
    const root = BASS_ROOTS[i] ?? 36;
    out.push(addNote(nextId(), bassRegion, root, i * bar, beat * 2, BASS_VELOCITY));
    out.push(addNote(nextId(), bassRegion, root, i * bar + beat * 2, beat * 2 - beat / 4, BASS_VELOCITY));
  });
  return out;
}

const STRESS_STEP = 120;
const STRESS_DURATION = 240;
const STRESS_LOW = 36;
const STRESS_SPAN = 60;
const LCG_MUL = 1103515245;
const LCG_ADD = 12345;
const LCG_MOD = 2 ** 31;

/**
 * Setup commands plus note groups of at most `chunk` commands. One track and one region by
 * default; `tracks` and `regions` spread the same notes over a bench shaped project (regions go
 * round robin over the tracks and sit one after another on their lane).
 */
export function stressCommands(
  count: number,
  nextId: () => string,
  chunk = config.stressChunk,
  tracks = 1,
  regions = 1,
): { setup: Command[]; groups: Command[][] } {
  const trackCount = Math.max(1, Math.floor(tracks));
  const regionCount = Math.max(trackCount, Math.floor(regions));
  const perRegion = Math.max(1, Math.ceil(count / regionCount));
  const regionLength = perRegion * STRESS_STEP + STRESS_DURATION;
  const trackIds = Array.from({ length: trackCount }, () => nextId());
  const setup: Command[] = [{ command: 'set_project_name', name: `Stress ${count}` }];
  trackIds.forEach((id, i) => {
    setup.push({ command: 'add_track', id, name: trackCount === 1 ? 'Stress' : `Stress ${i + 1}`, kind: 'instrument', index: null });
    setup.push({ command: 'set_instrument', track: id, instrument: INSTRUMENT_SINE });
  });
  const regionIds: string[] = [];
  for (let r = 0; r < regionCount; r++) {
    const id = nextId();
    regionIds.push(id);
    const lane = r % trackCount;
    const slot = Math.floor(r / trackCount);
    setup.push({ command: 'add_region', id, track: trackIds[lane] ?? '', start: slot * regionLength, length: regionLength, kind: 'midi' });
  }
  const groups: Command[][] = [];
  let seed = 7;
  let current: Command[] = [];
  for (let i = 0; i < count; i++) {
    seed = (seed * LCG_MUL + LCG_ADD) % LCG_MOD;
    const pitch = STRESS_LOW + (seed % STRESS_SPAN);
    const r = Math.min(regionCount - 1, Math.floor(i / perRegion));
    current.push(addNote(nextId(), regionIds[r] ?? '', pitch, (i - r * perRegion) * STRESS_STEP, STRESS_DURATION));
    if (current.length >= chunk) {
      groups.push(current);
      current = [];
    }
  }
  if (current.length > 0) groups.push(current);
  return { setup, groups };
}
