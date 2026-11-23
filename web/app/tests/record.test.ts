import { describe, expect, it } from 'vitest';
import type { AudioEngine } from '../src/audio/audioEngine';
import { starterCommands } from '../src/fixtures';
import { matchShortcut } from '../src/input/keymap';
import { locate, recordCommands, TakeBuffer } from '../src/input/recorder';
import type { CapturePos } from '../src/input/recorder';
import type { Command, TempoMap } from '../src/model';
import { RecordActions, type RecordAudio } from '../src/state/recordActions';
import { setTempo } from '../src/state/commands';
import { findRegion, sortedNotes, Store } from '../src/state/store';
import { TransportActions } from '../src/state/transportActions';
import { ticksToSeconds } from '../src/time';
import { counterIds, newActions, newBackend } from './helpers';

const BAR = 3840;
const mapAt = (bpm: number): TempoMap => ({ tempos: { '0': bpm * 1000 }, meters: { '0': { numerator: 4, denominator: 4 } } });

/** The audio engine with a clock the test moves by hand: a 1 bar count in, then the song plays. */
class FakeAudio implements RecordAudio {
  status = { transport: 'stopped' as 'stopped' | 'playing' | 'paused', counting: false };
  clock = 100;
  playTick = 0;
  private anchor: { clock: number; tick: number } | null = null;
  private countEnd = 0;
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly store: Store,
    private readonly countBars: number,
  ) {}

  subscribe(l: () => void): () => void {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  }

  private emit(): void {
    for (const l of this.listeners) l();
  }

  private map(): TempoMap {
    return this.store.getSnapshot().project.tempo_map;
  }

  async play(): Promise<void> {
    this.status.transport = 'playing';
    if (this.countBars > 0) {
      this.status.counting = true;
      this.countEnd = this.clock + ticksToSeconds(this.map(), this.countBars * BAR);
    } else {
      this.anchor = { clock: this.clock, tick: this.playTick };
    }
    this.emit();
  }

  pause(): void {
    this.status.transport = 'paused';
    this.emit();
  }

  /** Moves the clock; the count in ends on its own, like the worklet's report would say. */
  advance(seconds: number): void {
    this.clock += seconds;
    if (this.status.counting && this.clock >= this.countEnd) {
      this.status.counting = false;
      this.anchor = { clock: this.countEnd, tick: this.playTick };
    }
    this.emit();
  }

  capturePosition(afterStop = false): CapturePos | null {
    if (!this.anchor || (!afterStop && this.status.transport !== 'playing')) return null;
    const l = this.store.getSnapshot().transport.loop;
    return locate(this.map(), { clock: this.anchor.clock, tick: this.anchor.tick, pass: 0 }, this.clock, l);
  }
}

async function rig(bpm: number, countBars: number) {
  const store = new Store(newBackend());
  await store.refresh();
  await store.group('starter', starterCommands(counterIds()));
  await store.apply(setTempo(bpm));
  const actions = newActions(store);
  const audio = new FakeAudio(store, countBars);
  const transport = new TransportActions(store, audio as unknown as AudioEngine, () => undefined);
  const record = new RecordActions(store, audio, actions, transport, () => undefined);
  const snap = store.getSnapshot();
  const region = findRegion(snap.project, snap.selectedRegion)!.region;
  return { store, audio, record, regionId: region.id, base: sortedNotes(region).length };
}

const notesNow = (store: Store, regionId: string) => sortedNotes(findRegion(store.getSnapshot().project, regionId)!.region);

describe('recording math', () => {
  it('places a press on the tick for the tempo, at two tempos', () => {
    // One beat is 960 ticks: 0.5 s at 120 bpm and 1 s at 60 bpm.
    for (const [bpm, beatSeconds] of [
      [120, 0.5],
      [60, 1],
    ] as const) {
      const anchor = { clock: 10, tick: 0, pass: 0 };
      const off = { on: false, start: 0, end: 0 };
      expect(locate(mapAt(bpm), anchor, 10 + beatSeconds, off).tick).toBeCloseTo(960, 6);
      expect(locate(mapAt(bpm), anchor, 10 + 2.5 * beatSeconds, off).tick).toBeCloseTo(2400, 6);
    }
  });

  it('counts loop passes and folds the tick back into the loop', () => {
    const loop = { on: true, start: 0, end: BAR };
    const anchor = { clock: 0, tick: 0, pass: 4 };
    // At 120 bpm a bar is 2 s.
    expect(locate(mapAt(120), anchor, 1, loop)).toMatchObject({ pass: 4, tick: 1920 });
    const next = locate(mapAt(120), anchor, 2.5, loop);
    expect(next.pass).toBe(5);
    expect(next.tick).toBeCloseTo(960, 6);
    expect(locate(mapAt(120), anchor, 4.5, loop).pass).toBe(6);
  });

  it('splits a note held across the loop end', () => {
    const buf = new TakeBuffer({ start: 0, end: BAR });
    buf.noteOn(60, 90, { tick: 3000, pass: 1 });
    buf.noteOff(60, { tick: 500, pass: 2 });
    expect(buf.finish({ tick: 600, pass: 2 })).toEqual([
      { pitch: 60, velocity: 90, start: 3000, duration: 840, pass: 1 },
      { pitch: 60, velocity: 90, start: 0, duration: 500, pass: 2 },
    ]);
  });

  it('tidies to the grid only when asked, and grows the clip for a take that runs long', () => {
    const takes = [{ pitch: 60, velocity: 100, start: 530, duration: 100, pass: 0 }];
    const target = { region: 'r', start: 0, length: 480 };
    let n = 0;
    const newId = (): string => String(++n);
    const tidy = recordCommands(takes, target, { grid: 480, bar: BAR, newId });
    const loose = recordCommands(takes, target, { grid: null, bar: BAR, newId });
    const note = (c: Command[]) => {
      const add = c.find((x) => x.command === 'add_notes');
      return add?.command === 'add_notes' ? add.notes[0] : undefined;
    };
    expect(note(tidy)).toMatchObject({ start: 480, duration: 480 });
    expect(note(loose)).toMatchObject({ start: 530, duration: 100 });
    expect(tidy[0]).toEqual({ command: 'resize_region', region: 'r', length: BAR });
    expect(loose.some((c) => c.command === 'resize_region')).toBe(true);
  });
});

describe('RecordActions with a fake audio clock', () => {
  for (const bpm of [120, 60]) {
    it(`writes four presses as one undo step at ${bpm} bpm`, async () => {
      const { store, audio, record, regionId, base } = await rig(bpm, 1);
      const beat = 60 / bpm;
      const depth = store.getSnapshot().undoDepth;
      await record.start();
      expect(record.getSnapshot().phase).toBe('counting');
      // A press during the count in is not part of the take.
      record.noteOn(40, 100);
      audio.advance(4 * beat);
      expect(record.getSnapshot().phase).toBe('recording');
      // Played a little late each time (+30 ms) so tidy timing has something to do.
      const pitches = [60, 62, 64, 65];
      for (const pitch of pitches) {
        record.noteOn(pitch, 90);
        audio.advance(0.03);
        record.noteOff(pitch);
        audio.advance(beat - 0.03);
      }
      audio.pause();
      await expect.poll(() => notesNow(store, regionId).length).toBe(base + 4);
      const added = notesNow(store, regionId).filter((n) => n.velocity === 90);
      // Tidy timing is on at eighth notes (480 ticks), so every start is on the grid.
      expect(added.map((n) => n.start).sort((a, b) => a - b)).toEqual([0, 960, 1920, 2880]);
      expect(store.getSnapshot().undoDepth).toBe(depth + 1);
      expect(store.stepName(depth + 1)).toBe('Record 4 notes');
      await store.undo();
      expect(notesNow(store, regionId)).toHaveLength(base);
    });
  }

  it('keeps the played timing when tidy timing is off', async () => {
    const { store, audio, record, regionId, base } = await rig(120, 0);
    record.setTidy(false);
    await record.start();
    expect(record.getSnapshot().phase).toBe('recording');
    audio.advance(0.5 + 0.03);
    record.noteOn(60, 100);
    audio.advance(0.1);
    record.noteOff(60);
    audio.pause();
    await expect.poll(() => notesNow(store, regionId).length).toBe(base + 1);
    const n = notesNow(store, regionId).find((x) => x.pitch === 60 && x.start > 0 && x.start < 2000)!;
    // 0.53 s at 120 bpm is 1017.6 ticks, played length 0.1 s is 192 ticks.
    expect(n.start).toBe(1018);
    expect(n.duration).toBe(192);
  });

  it('records each loop pass as its own step and Undo takes back the last pass only', async () => {
    const { store, audio, record, regionId, base } = await rig(120, 0);
    await store.setTransport({ loop: { on: true, start: 0, end: BAR } });
    const depth = store.getSnapshot().undoDepth;
    await record.start();
    audio.advance(0.5);
    record.noteOn(60, 100);
    audio.advance(0.1);
    record.noteOff(60);
    // The loop is 2 s long; the next press is in pass two.
    audio.advance(2);
    record.noteOn(64, 100);
    audio.advance(0.1);
    record.noteOff(64);
    audio.pause();
    await expect.poll(() => store.getSnapshot().undoDepth).toBe(depth + 2);
    expect(notesNow(store, regionId)).toHaveLength(base + 2);
    await store.undo();
    const left = notesNow(store, regionId);
    expect(left).toHaveLength(base + 1);
    expect(left.some((n) => n.pitch === 60 && n.start < 2000)).toBe(true);
    expect(left.some((n) => n.pitch === 64 && n.start > 0 && n.start < 2000)).toBe(false);
  });

  it('writes nothing when no key was played', async () => {
    const { store, audio, record } = await rig(120, 0);
    const depth = store.getSnapshot().undoDepth;
    await record.start();
    audio.advance(1);
    audio.pause();
    await new Promise((r) => setTimeout(r, 20));
    expect(record.getSnapshot().phase).toBe('idle');
    expect(store.getSnapshot().undoDepth).toBe(depth);
  });
});

describe('record keys', () => {
  const key = (init: Partial<KeyboardEvent>): KeyboardEvent => ({ ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, code: '', key: '', ...init }) as KeyboardEvent;

  it('maps R to record and K or Shift K to the Keys toggle, and leaves modified keys alone', () => {
    expect(matchShortcut(key({ key: 'r', code: 'KeyR' }))).toBe('record');
    expect(matchShortcut(key({ key: 'k', code: 'KeyK' }))).toBe('keys');
    expect(matchShortcut(key({ key: 'K', code: 'KeyK', shiftKey: true }))).toBe('keys');
    expect(matchShortcut(key({ key: 'r', code: 'KeyR', ctrlKey: true }))).toBeNull();
  });
});
