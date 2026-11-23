// Tap tempo: the average gap between the last few taps, restarted after a long pause.
// Pure so the timing rules can be tested without a clock.

export interface TapOptions {
  /** A gap longer than this starts a new count. */
  resetMs: number;
  /** Taps kept for the average. */
  window: number;
  /** Taps needed before a tempo is reported (two taps make one gap). */
  minTaps: number;
}

export class TapTempo {
  private taps: number[] = [];

  constructor(private readonly opts: TapOptions) {}

  /** Records a tap at `nowMs` and returns the average tempo in BPM, or null while there are too few taps. */
  tap(nowMs: number): number | null {
    const last = this.taps[this.taps.length - 1];
    if (last !== undefined && nowMs - last > this.opts.resetMs) this.taps = [];
    // A repeated timestamp would be a zero gap and an infinite tempo.
    if (last !== undefined && nowMs <= last && this.taps.length > 0) return this.bpm();
    this.taps.push(nowMs);
    const keep = Math.max(2, this.opts.window);
    if (this.taps.length > keep) this.taps.splice(0, this.taps.length - keep);
    return this.bpm();
  }

  reset(): void {
    this.taps = [];
  }

  count(): number {
    return this.taps.length;
  }

  private bpm(): number | null {
    if (this.taps.length < Math.max(2, this.opts.minTaps)) return null;
    const first = this.taps[0] as number;
    const last = this.taps[this.taps.length - 1] as number;
    const gap = (last - first) / (this.taps.length - 1);
    return gap > 0 ? 60000 / gap : null;
  }
}
