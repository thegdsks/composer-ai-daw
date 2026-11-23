// Catch what I just played: notes from any live source go into a rolling buffer, and Catch
// writes them as one new clip with the existing commands (add_region, add_notes), one group.
import { CatchBuffer, catchPlan, freeClipStart } from '../input/catchBuffer';
import { newId } from '../ids';
import type { Id } from '../model';
import { meterSegments, secondsToTicks, snapFloor, ticksPerBar } from '../time';
import type { Store } from './store';

export interface CatchState {
  /** Notes in the buffer now. */
  count: number;
}

export interface CatchEnv {
  /** Wall clock in milliseconds. */
  now(): number;
  /** Playhead in ticks. */
  playTick(): number;
  /** Grid ticks to tidy to, or null to keep the played timing. */
  grid(): number | null;
  limits: { windowMs: number; maxNotes: number };
}

export class CatchActions {
  private readonly buffer: CatchBuffer;
  private state: CatchState = { count: 0 };
  private readonly listeners = new Set<() => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly store: Store,
    private readonly env: CatchEnv,
    private readonly notify: (message: string) => void,
  ) {
    this.buffer = new CatchBuffer(env.limits);
  }

  dispose(): void {
    if (this.timer) clearTimeout(this.timer);
  }

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  getSnapshot = (): CatchState => this.state;

  private refresh(): void {
    const count = this.buffer.count(this.env.now());
    if (this.timer) clearTimeout(this.timer);
    // The buffer empties by itself once nothing is newer than the window, so the button dims again.
    this.timer = count > 0 ? setTimeout(() => this.refresh(), this.env.limits.windowMs / 4 + 1) : null;
    if (count === this.state.count) return;
    this.state = { count };
    for (const l of this.listeners) l();
  }

  noteOn(pitch: number, velocity: number): void {
    this.buffer.noteOn(pitch, velocity, this.env.now(), this.store.getSnapshot().selectedTrack);
    this.refresh();
  }

  noteOff(pitch: number): void {
    this.buffer.noteOff(pitch, this.env.now());
    this.refresh();
  }

  /** Writes the buffer as a new clip on the track of the latest note, at the playhead bar. Returns the note count or 0. */
  async commit(): Promise<number> {
    const now = this.env.now();
    const played = this.buffer.notes(now);
    const snap = this.store.getSnapshot();
    const last = played[played.length - 1];
    const track = snap.project.tracks.find((t) => t.id === last?.track) ?? snap.project.tracks.find((t) => t.id === snap.selectedTrack);
    if (!last || !track) {
      this.notify('Nothing to catch yet. Play some notes first.');
      return 0;
    }
    const map = snap.project.tempo_map;
    const bar = ticksPerBar(meterSegments(map)[0]?.sig ?? { numerator: 4, denominator: 4 });
    const spanMs = Math.max(...played.map((n) => n.end ?? n.start)) - Math.min(...played.map((n) => n.start));
    const approxTicks = secondsToTicks(map, spanMs / 1000);
    const length = Math.max(bar, Math.ceil(approxTicks / bar) * bar);
    const clipStart = freeClipStart(Object.values(track.regions), snapFloor(this.env.playTick(), bar), length, bar);
    const region: Id = newId();
    const cmds = catchPlan(played, { map, track: track.id, region, clipStart, bar, grid: this.env.grid() });
    const r = await this.store.group(`Catch ${played.length} ${played.length === 1 ? 'note' : 'notes'}`, cmds);
    if (!r.ok) {
      this.notify('Could not make a clip from those notes.');
      return 0;
    }
    this.buffer.clear();
    this.refresh();
    this.store.selectRegion(track.id, region);
    return played.length;
  }
}
