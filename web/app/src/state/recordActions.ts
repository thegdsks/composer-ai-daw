// Record from the computer keyboard or MIDI. A take is captured on the audio clock and written
// with the existing note commands, one group per loop pass (D31: no new command).
import type { CapturePos } from '../input/recorder';
import { recordCommands, recordLabel, TakeBuffer, type Take } from '../input/recorder';
import type { AudioStatus } from '../audio/audioEngine';
import { config, gridChoices } from '../config';
import { isMidi, type Id } from '../model';
import { getPrefs, setPrefs } from '../prefs';
import { loopActive, meterSegments, snapFloor, ticksPerBar } from '../time';
import { findRegion, type Store } from './store';
import type { Actions } from './actions';
import type { TransportActions } from './transportActions';

/** What recording needs from the audio engine (a fake clock stands in for it in tests). */
export interface RecordAudio {
  readonly status: Pick<AudioStatus, 'transport' | 'counting'>;
  readonly playTick: number;
  subscribe(l: () => void): () => void;
  play(forceCountIn?: boolean): Promise<void>;
  pause(): void;
  capturePosition(afterStop?: boolean): CapturePos | null;
}

export type RecordPhase = 'idle' | 'starting' | 'counting' | 'recording';

export interface RecordState {
  phase: RecordPhase;
  /** Snap recorded notes to the grid. */
  tidy: boolean;
  gridId: string;
  /** The computer keyboard plays notes (the Keys setting). It is also on while a recording runs. */
  pianoKeys: boolean;
  /** Notes written by the last take, for the status line. */
  lastNotes: number;
  /** Start ticks (inside the clip) of the notes the last take wrote, for the debug probe. */
  lastStarts: number[];
}

function defaultGridId(): string {
  const id = `1/${config.recordGridDenominator}`;
  return gridChoices().some((c) => c.id === id) ? id : (gridChoices()[0]?.id ?? id);
}

export class RecordActions {
  private state: RecordState = { phase: 'idle', tidy: true, gridId: defaultGridId(), pianoKeys: getPrefs().pianoKeys, lastNotes: 0, lastStarts: [] };
  private readonly listeners = new Set<() => void>();
  private buffer: TakeBuffer | null = null;
  private region: Id | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private queue: Promise<void> = Promise.resolve();
  private readonly off: () => void;

  constructor(
    private readonly store: Store,
    private readonly audio: RecordAudio,
    private readonly actions: Actions,
    private readonly transport: TransportActions,
    private readonly notify: (message: string) => void,
  ) {
    this.off = audio.subscribe(() => this.check());
  }

  dispose(): void {
    this.off();
    this.stopTimer();
  }

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  getSnapshot = (): RecordState => this.state;

  private set(patch: Partial<RecordState>): void {
    this.state = { ...this.state, ...patch };
    for (const l of this.listeners) l();
  }

  /** The computer keyboard plays notes only while Keys is on or a recording is armed. */
  get keysActive(): boolean {
    return this.state.pianoKeys || this.state.phase !== 'idle';
  }

  setPianoKeys(on: boolean): void {
    setPrefs({ pianoKeys: on });
    this.set({ pianoKeys: on });
    this.store.say(on ? 'Keys on. A to semicolon play notes' : 'Keys off');
  }

  togglePianoKeys(): void {
    this.setPianoKeys(!this.state.pianoKeys);
  }

  setTidy(on: boolean): void {
    this.set({ tidy: on });
    this.store.say(on ? 'Tidy timing on' : 'Tidy timing off');
  }

  setGrid(gridId: string): void {
    if (gridChoices().some((c) => c.id === gridId)) this.set({ gridId });
  }

  private gridTicks(): number | null {
    if (!this.state.tidy) return null;
    return gridChoices().find((c) => c.id === this.state.gridId)?.ticks ?? null;
  }

  async toggle(): Promise<void> {
    if (this.state.phase === 'idle') await this.start();
    else this.stop();
  }

  /** The selected clip, else a new clip at the playhead on the selected track (a new track when there are none). */
  private async ensureTarget(): Promise<Id | null> {
    const snap = this.store.getSnapshot();
    const found = findRegion(snap.project, snap.selectedRegion);
    if (found && isMidi(found.region)) return found.region.id;
    if (snap.project.tracks.length === 0) await this.actions.addTrack();
    const now = this.store.getSnapshot();
    const track = now.project.tracks.find((t) => t.id === now.selectedTrack) ?? now.project.tracks[0];
    if (!track) return null;
    const existing = findRegion(now.project, now.selectedRegion);
    if (existing && isMidi(existing.region)) return existing.region.id;
    const bar = ticksPerBar(meterSegments(now.project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 });
    return this.actions.addRegion(track.id, snapFloor(this.audio.playTick, bar));
  }

  async start(): Promise<void> {
    if (this.state.phase !== 'idle') return;
    this.set({ phase: 'starting' });
    const region = await this.ensureTarget();
    const found = findRegion(this.store.getSnapshot().project, region);
    if (!region || !found) {
      this.set({ phase: 'idle' });
      this.notify('Add a track first, then press Record.');
      return;
    }
    this.region = region;
    this.set({ lastNotes: 0, lastStarts: [] });
    const snap = this.store.getSnapshot();
    const loop = snap.transport.loop;
    const looping = loopActive(loop);
    this.buffer = new TakeBuffer(looping ? { start: loop.start, end: loop.end } : null);
    // Start inside the clip (or the loop) so the first notes land where the player sees the clip.
    if (this.audio.status.transport !== 'playing') {
      const tick = this.audio.playTick;
      const lo = looping ? loop.start : found.region.start;
      const hi = looping ? loop.end : found.region.start + found.region.length;
      if (tick < lo || tick >= hi) await this.transport.seek(lo);
    }
    await this.audio.play(true);
    if (this.audio.status.transport !== 'playing') {
      this.reset();
      this.notify('Sound did not start, so nothing is recording.');
      return;
    }
    const counting = this.audio.status.counting;
    this.set({ phase: counting ? 'counting' : 'recording' });
    this.store.say(counting ? 'Counting in, then recording' : 'Recording');
    this.timer = setInterval(() => this.check(), config.recordPollMs);
  }

  stop(): void {
    if (this.state.phase === 'idle') return;
    if (this.audio.status.transport === 'playing') this.audio.pause();
    else void this.finish();
  }

  private stopTimer(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  private reset(): void {
    this.stopTimer();
    this.buffer = null;
    this.region = null;
    this.set({ phase: 'idle' });
  }

  /** Runs on every audio change and on a short timer: count in to recording, finished loop passes, and stop. */
  private check(): void {
    const phase = this.state.phase;
    if (phase === 'idle' || phase === 'starting') return;
    if (this.audio.status.transport !== 'playing') {
      // Stopped (not paused) means the song ran out, which a person did not ask for.
      if (this.audio.status.transport === 'stopped') this.notify('Recording stopped at the end of the song. Turn Loop on to keep going.');
      void this.finish();
      return;
    }
    if (phase === 'counting' && !this.audio.status.counting) {
      this.set({ phase: 'recording' });
      this.store.say('Recording');
    }
    const pos = this.state.phase === 'recording' ? this.audio.capturePosition() : null;
    if (pos && this.buffer) {
      this.buffer.advance(pos);
      this.write(this.buffer.takeBefore(pos.pass), this.region);
    }
  }

  private async finish(): Promise<void> {
    const buffer = this.buffer;
    const region = this.region;
    const pos = this.audio.capturePosition(true);
    this.reset();
    if (!buffer) return;
    const takes = pos ? buffer.finish(pos) : [];
    this.write(takes, region);
    await this.queue;
    if (takes.length === 0) this.store.say('Nothing was played, so nothing was recorded');
  }

  /** One group per loop pass, in order, so Undo takes back the last pass only. */
  private write(takes: Take[], region: Id | null): void {
    if (takes.length === 0) return;
    const passes = new Map<number, Take[]>();
    for (const t of takes) passes.set(t.pass, [...(passes.get(t.pass) ?? []), t]);
    const grid = this.gridTicks();
    for (const pass of [...passes.keys()].sort((a, b) => a - b)) {
      const list = passes.get(pass) ?? [];
      this.queue = this.queue.then(async () => {
        const snap = this.store.getSnapshot();
        const found = findRegion(snap.project, region);
        if (!region || !found) {
          this.notify('The clip is gone, so the take was dropped.');
          return;
        }
        const bar = ticksPerBar(meterSegments(snap.project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 });
        const cmds = recordCommands(list, { region, start: found.region.start, length: found.region.length }, { grid, bar });
        const noteCount = cmds.reduce((n, c) => n + (c.command === 'add_notes' ? c.notes.length : 0), 0);
        const r = await this.store.group(recordLabel(noteCount), cmds);
        if (r.ok) this.set({ lastNotes: noteCount, lastStarts: cmds.flatMap((c) => (c.command === 'add_notes' ? c.notes.map((n) => n.start) : [])) });
        else this.notify('Could not write the recorded notes.');
      });
    }
  }

  /** Live note on from the computer keyboard, MIDI or the roll keys. */
  noteOn(pitch: number, velocity: number): void {
    if (this.state.phase !== 'recording' || !this.buffer) return;
    const pos = this.audio.capturePosition();
    if (pos) this.buffer.noteOn(pitch, velocity, pos);
  }

  noteOff(pitch: number): void {
    if (this.state.phase !== 'recording' || !this.buffer) return;
    const pos = this.audio.capturePosition();
    if (pos) this.buffer.noteOff(pitch, pos);
  }
}
