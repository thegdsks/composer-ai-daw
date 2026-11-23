// Transport session state (loop, metronome, count in, follow) and the playhead. Everything goes
// through transport.set on the backend, so the agent sees the same state; none of it is undoable.
import type { AudioEngine } from '../audio/audioEngine';
import { config } from '../config';
import type { Id } from '../model';
import { barRangeText, meterSegments, ticksPerBar } from '../time';
import { findRegion, projectEnd, type Store } from './store';

export class TransportActions {
  constructor(
    private readonly store: Store,
    private readonly audio: AudioEngine,
    private readonly notify: (message: string) => void,
  ) {}

  private get snap() {
    return this.store.getSnapshot();
  }

  /** The range of the selected clips, else the first clip, else the whole song; null when empty. */
  private selectionRange(): { start: number; end: number } | null {
    const snap = this.snap;
    let start = Infinity;
    let end = 0;
    const ids: Id[] = [...snap.selectedRegions];
    if (ids.length === 0 && snap.selectedRegion) ids.push(snap.selectedRegion);
    for (const id of ids) {
      const f = findRegion(snap.project, id);
      if (!f) continue;
      start = Math.min(start, f.region.start);
      end = Math.max(end, f.region.start + f.region.length);
    }
    if (end > start) return { start, end };
    const total = projectEnd(snap.project);
    return total > 0 ? { start: 0, end: total } : null;
  }

  /** Sets the loop range; `on` defaults to switching the loop on. Ranges are in ticks. */
  async setLoop(start: number, end: number, on = true): Promise<boolean> {
    if (!(end > start)) return false;
    const ok = await this.store.setTransport({ loop: { on, start: Math.max(0, Math.round(start)), end: Math.round(end) } });
    if (ok && on) this.store.say(`Loop on, ${barRangeText(this.snap.project.tempo_map, start, end)}`);
    return ok;
  }

  /** Loops the selected clips (or the first clip, or the song). Templates call this with on false or true. */
  async loopSelection(on = true): Promise<boolean> {
    const r = this.selectionRange();
    if (!r) {
      if (on) this.notify('Add a clip first, then loop it.');
      return false;
    }
    return this.setLoop(r.start, r.end, on);
  }

  async toggleLoop(): Promise<void> {
    const loop = this.snap.transport.loop;
    if (loop.on) {
      if (await this.store.setTransport({ loop: { on: false } })) this.store.say('Loop off');
      return;
    }
    if (loop.end > loop.start) await this.setLoop(loop.start, loop.end, true);
    else await this.loopSelection(true);
  }

  async toggleMetronome(): Promise<void> {
    const on = !this.snap.transport.metronome.on;
    if (await this.store.setTransport({ metronome: { on } })) this.store.say(on ? 'Metronome on' : 'Metronome off');
  }

  async setCountIn(bars: number): Promise<void> {
    const n = Math.min(config.countInMaxBars, Math.max(0, Math.round(bars)));
    if (await this.store.setTransport({ countInBars: n })) this.store.say(n === 0 ? 'Count in off' : `Count in ${n} ${n === 1 ? 'bar' : 'bars'}`);
  }

  async toggleFollow(): Promise<void> {
    const follow = !this.snap.transport.follow;
    if (await this.store.setTransport({ follow })) this.store.say(follow ? 'Follow on' : 'Follow off');
  }

  /** Moves the playhead; the backend keeps the same tick so the agent can read it. */
  async seek(tick: number): Promise<void> {
    const t = Math.max(0, Math.round(tick));
    await this.audio.seekTick(t);
    void this.store.backend.transportSet({ tick: t });
  }

  /** Playhead one bar earlier or later (bars follow the first meter). */
  async seekBars(delta: number): Promise<void> {
    const bar = ticksPerBar(meterSegments(this.snap.project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 });
    const now = this.audio.playTick;
    const target = Math.max(0, Math.round(now / bar + delta) * bar);
    await this.seek(target);
    this.store.say(`Playhead at ${barRangeText(this.snap.project.tempo_map, target, target + 1)}`);
  }
}
