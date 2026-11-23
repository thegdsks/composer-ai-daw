// Metronome and count in click: a short decaying sine, generated here instead of in the engine
// so it never touches the song, the project hash or the exported audio. Deterministic: the same
// start frames always give the same samples.

interface Voice {
  /** Samples still to wait inside the current render call before the click starts. */
  delay: number;
  age: number;
  accent: boolean;
}

export interface ClickOptions {
  sampleRate: number;
  ms: number;
  gain: number;
  hz: number;
  accentHz: number;
}

export class ClickBank {
  private voices: Voice[] = [];
  private readonly length: number;
  private readonly decay: number;
  private started = 0;
  private top = 0;

  constructor(private readonly o: ClickOptions) {
    this.length = Math.max(1, Math.round((o.ms / 1000) * o.sampleRate));
    // The envelope falls to about 0.7 percent at the end of the click.
    this.decay = this.length / 5;
  }

  /** Clicks started since creation, for the debug panel. */
  get count(): number {
    return this.started;
  }

  /** Loudest click sample rendered so far, so a driver can tell the click really sounded. */
  get peak(): number {
    return this.top;
  }

  /** Schedules a click `delay` samples into the next `render` call. */
  start(delay: number, accent: boolean): void {
    this.voices.push({ delay: Math.max(0, Math.round(delay)), age: 0, accent });
    this.started++;
  }

  /** Adds `count` samples of every sounding click to both channels from `offset`. */
  render(left: Float32Array, right: Float32Array, offset: number, count: number): void {
    if (this.voices.length === 0) return;
    const { sampleRate, gain, hz, accentHz } = this.o;
    for (const v of this.voices) {
      const w = (2 * Math.PI * (v.accent ? accentHz : hz)) / sampleRate;
      const level = v.accent ? gain : gain * 0.75;
      for (let i = 0; i < count && v.age < this.length; i++) {
        if (v.delay > 0) {
          v.delay--;
          continue;
        }
        const s = Math.sin(w * v.age) * Math.exp(-v.age / this.decay) * level;
        this.top = Math.max(this.top, Math.abs(s));
        left[offset + i] = (left[offset + i] ?? 0) + s;
        right[offset + i] = (right[offset + i] ?? 0) + s;
        v.age++;
      }
    }
    this.voices = this.voices.filter((v) => v.age < this.length);
  }

  clear(): void {
    this.voices = [];
  }
}
