// Audio thread: two wasm engines. `song` plays the compiled project; `live` has the same
// tracks but no events, so keyboard and MIDI notes sound even while the transport is stopped.
import { WasmEngine, BLOCK_FRAMES, MAX_PEAK_TRACKS } from '../wasm/engine';
import { liveOnlyBlob, sameBytes, trackTable } from '../wasm/blob';
import { lowerBound, segmentFrames } from '../time';
import { ClickBank } from './click';
import { WclapError } from './wclap/inspect';
import { WclapPlugin } from './wclap/pluginHost';
import type { CountIn, FromWorklet, ToWorklet, TransportPlan, WorkletOptions } from './protocol';

declare global {
  class AudioWorkletProcessor {
    readonly port: MessagePort;
    constructor(options?: unknown);
  }
  const sampleRate: number;
  const currentTime: number;
  function registerProcessor(
    name: string,
    ctor: new (options: { processorOptions: WorkletOptions }) => AudioWorkletProcessor,
  ): void;
}

class ComposerProcessor extends AudioWorkletProcessor {
  private song: WasmEngine;
  /** A second song engine, caught up to the loop start or a seek target while the first one plays. */
  private spare: WasmEngine;
  private readonly live: WasmEngine;
  /** Sound browser audition; created on first use so most sessions never pay for it. */
  private preview: WasmEngine | null = null;
  private readonly module: WebAssembly.Module;
  private songBlob: Uint8Array | null = null;
  private liveTracks: Uint8Array | null = null;
  private playing = false;
  private sinceReport = 0;
  private peak = 0;
  /** Per track peaks of song and live engines, merged until the next report. */
  private readonly trackPeaks = new Float32Array(MAX_PEAK_TRACKS);
  private trackCount = 0;
  private readonly reportEvery: number;
  private readonly click: ClickBank;
  private readonly budget: number;
  private plan: TransportPlan = { loop: { on: false, startFrame: 0, endFrame: 0 }, metronome: false, beats: new Float64Array(0), accents: new Uint8Array(0) };
  private countIn: CountIn | null = null;
  private countDone = 0;
  /** The spare is being brought to `target`; `seek` says a swap into the song follows. */
  private prep: { target: number; done: number; seekId: number | null } | null = null;
  /** Frame the finished spare sits at, or null when it is not ready. */
  private spareAt: number | null = null;
  private seeks = 0;
  private wraps = 0;
  /** Web plugin inserted after the mix; its output is never part of the project hash or the offline render. */
  private plugin: WclapPlugin | null = null;
  private pluginBlocks = 0;
  private bypassSent = false;
  private readonly wclap: WorkletOptions['wclap'];

  constructor(options: { processorOptions: WorkletOptions }) {
    super();
    const module = new WebAssembly.Module(options.processorOptions.wasm);
    this.module = module;
    this.song = WasmEngine.fromModule(module);
    this.spare = WasmEngine.fromModule(module);
    const o = options.processorOptions;
    this.click = new ClickBank({ sampleRate, ms: o.clickMs, gain: o.clickGain, hz: o.clickHz, accentHz: o.clickAccentHz });
    this.budget = Math.max(128, o.seekBudgetFrames);
    this.live = WasmEngine.fromModule(module);
    this.reportEvery = options.processorOptions.positionReportFrames;
    this.wclap = o.wclap;
    this.port.onmessage = (e: MessageEvent<ToWorklet>) => this.onMessage(e.data);
    this.send({ type: 'ready' });
  }

  private send(m: FromWorklet): void {
    this.port.postMessage(m);
  }

  private onMessage(m: ToWorklet): void {
    try {
      switch (m.type) {
        case 'load':
          this.load(m.blob, m.keepPosition, m.maxSeekFrames);
          break;
        case 'play':
          this.playing = this.song.loaded;
          this.countIn = this.playing && m.countIn && m.countIn.totalFrames > 0 ? m.countIn : null;
          this.countDone = 0;
          break;
        case 'pause':
          this.playing = false;
          this.countIn = null;
          break;
        case 'seek':
          this.beginPrep(Math.max(0, Math.round(m.frame)), m.id);
          break;
        case 'plan':
          this.plan = m.plan;
          // A moved loop start invalidates the spare; the next quantum rebuilds it.
          if (this.prep ? this.prep.seekId === null && this.prep.target !== m.plan.loop.startFrame : this.spareAt !== m.plan.loop.startFrame) {
            this.spareAt = null;
            this.prep = null;
          }
          break;
        case 'note':
          if (this.live.loaded) this.live.note(m.track, m.pitch, m.velocity, m.on);
          break;
        case 'preview-load':
          this.preview ??= WasmEngine.fromModule(this.module);
          this.preview.load(m.blob);
          break;
        case 'preview-note':
          if (this.preview?.loaded) this.preview.note(0, m.pitch, m.velocity, m.on);
          break;
        case 'wclap-load':
          this.loadPlugin(m.bytes, m.id);
          break;
        case 'wclap-param':
          this.plugin?.setParam(m.param, m.value);
          break;
        case 'wclap-unload':
          this.plugin?.destroy();
          this.plugin = null;
          break;
      }
    } catch (err) {
      this.send({ type: 'error', message: err instanceof Error ? err.message : 'worklet error' });
    }
  }

  private loadPlugin(bytes: Uint8Array, id: number): void {
    this.plugin?.destroy();
    this.plugin = null;
    try {
      const p = WclapPlugin.load(bytes, {
        sampleRate,
        limits: { maxFileBytes: this.wclap.maxFileBytes, maxMemoryPages: this.wclap.maxMemoryPages },
        maxEvents: this.wclap.maxEvents,
        budgetShare: this.wclap.budgetShare,
        overrunLimit: this.wclap.overrunLimit,
        // The worklet has no performance clock; Date is coarse, so a short overrun can hide.
        now: () => Date.now(),
      });
      this.plugin = p;
      this.pluginBlocks = 0;
      this.bypassSent = false;
      this.send({ type: 'wclap-loaded', id, info: { id: p.id, name: p.name, params: p.params, latencyFrames: p.latencyFrames } });
    } catch (e) {
      const code = e instanceof WclapError ? e.code : 'bad_module';
      this.send({ type: 'wclap-failed', id, code, message: e instanceof Error ? e.message : 'plugin failed to load' });
    }
  }

  private load(blob: Uint8Array, keep: boolean, maxSeek: number): void {
    const at = this.song.loaded ? this.song.position : 0;
    this.song.load(blob);
    this.songBlob = blob;
    if (keep && at > 0 && at <= maxSeek) this.song.skip(at);
    // The spare was built from the old blob.
    this.spareAt = null;
    if (this.prep) this.beginPrep(this.prep.target, this.prep.seekId);
    // Reloading the live engine cuts held notes, so only do it when the instruments change.
    const tracks = trackTable(blob).slice();
    if (!this.live.loaded || !this.liveTracks || !sameBytes(this.liveTracks, tracks)) {
      this.live.load(liveOnlyBlob(blob));
      this.liveTracks = tracks;
    }
  }

  private rewind(): void {
    if (this.songBlob && this.song.position > 0) this.song.load(this.songBlob);
  }

  private loopFrames(): { start: number; end: number } | null {
    const l = this.plan.loop;
    return l.on && l.endFrame > l.startFrame ? { start: l.startFrame, end: l.endFrame } : null;
  }

  private beginPrep(target: number, seekId: number | null): void {
    if (!this.songBlob) {
      if (seekId !== null) this.seeks = seekId;
      return;
    }
    this.spare.load(this.songBlob);
    this.spareAt = null;
    this.prep = { target, done: 0, seekId };
    this.stepPrep(0);
  }

  /** Catches the spare up by at most one budget of frames per audio quantum. */
  private stepPrep(frames: number): void {
    const p = this.prep;
    if (!p) {
      // Keep a spare waiting at the loop start so the wrap costs nothing.
      const loop = this.loopFrames();
      if (loop && this.songBlob && this.spareAt !== loop.start) this.beginPrep(loop.start, null);
      return;
    }
    const n = Math.min(frames, p.target - p.done);
    if (n > 0) this.spare.skip(n);
    p.done += Math.max(0, n);
    if (p.done < p.target) return;
    this.prep = null;
    this.spareAt = p.target;
    if (p.seekId !== null) {
      this.swapSpare();
      this.seeks = p.seekId;
      this.sinceReport = this.reportEvery;
    }
  }

  private swapSpare(): void {
    const t = this.song;
    this.song = this.spare;
    this.spare = t;
    this.spareAt = null;
    this.click.clear();
  }

  /** Jumps from the loop end back to its start on the exact sample. */
  private wrap(start: number): void {
    if (this.spareAt === start) this.swapSpare();
    else if (this.songBlob) {
      // The spare was not ready (a fresh edit or a late loop change): catch up here instead.
      this.song.load(this.songBlob);
      this.song.skip(start);
    }
    this.wraps++;
  }

  /** Plays the count in; returns how many frames of this quantum it used. */
  private runCountIn(n: number): number {
    const c = this.countIn;
    if (!c) return 0;
    const used = Math.min(n, c.totalFrames - this.countDone);
    const from = lowerBound(c.frames, this.countDone);
    for (let i = from; i < c.frames.length && (c.frames[i] as number) < this.countDone + used; i++) {
      this.click.start((c.frames[i] as number) - this.countDone, c.accents[i] === 1);
    }
    this.countDone += used;
    if (this.countDone >= c.totalFrames) this.countIn = null;
    return used;
  }

  private runSong(left: Float32Array, right: Float32Array, from: number, n: number): void {
    let off = from;
    while (off < n) {
      const loop = this.loopFrames();
      if (loop && this.song.position >= loop.end) this.wrap(loop.start);
      const pos = this.song.position;
      if (!loop && pos >= this.song.length) {
        this.playing = false;
        this.rewind();
        this.send({ type: 'ended' });
        return;
      }
      const take = segmentFrames(pos, n - off, loop ? loop.end : null);
      this.mix(this.song, left, right, off, take, true);
      if (this.plan.metronome) {
        const b = this.plan.beats;
        for (let i = lowerBound(b, pos); i < b.length && (b[i] as number) < pos + take; i++) {
          this.click.start(off + (b[i] as number) - pos, this.plan.accents[i] === 1);
        }
      }
      off += take;
    }
    // Land exactly on the loop start so a report never shows a playhead past the loop end.
    const loop = this.loopFrames();
    if (loop && this.song.position >= loop.end) this.wrap(loop.start);
  }

  process(_inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    const out = outputs[0];
    const left = out?.[0];
    if (!out || !left) return true;
    const right = out[1] ?? left;
    const n = left.length;
    try {
      this.stepPrep(this.budget);
      if (this.playing && this.song.loaded) {
        const used = this.runCountIn(n);
        this.runSong(left, right, used, n);
      }
      this.click.render(left, right, 0, n);
      if (this.live.loaded) this.mix(this.live, left, right, 0, n, true);
      if (this.preview?.loaded) this.mix(this.preview, left, right, 0, n);
      if (this.plugin) this.runPlugin(left, right, n);
    } catch (err) {
      this.send({ type: 'error', message: err instanceof Error ? err.message : 'render error' });
      return false;
    }
    for (let i = 0; i < n; i++) {
      const a = Math.max(Math.abs(left[i] ?? 0), Math.abs(right[i] ?? 0));
      if (a > this.peak) this.peak = a;
    }
    this.sinceReport += n;
    if (this.sinceReport >= this.reportEvery) {
      this.sinceReport = 0;
      this.send({
        type: 'pos',
        frames: this.song.loaded ? this.song.position : 0,
        at: currentTime + n / sampleRate,
        lengthFrames: this.song.loaded ? this.song.length : 0,
        playing: this.playing,
        counting: this.countIn !== null,
        seeks: this.seeks,
        wraps: this.wraps,
        clicks: this.click.count,
        clickPeak: this.click.peak,
        peak: this.peak,
        clipped: this.song.loaded ? this.song.clipped : 0,
        trackPeaks: this.trackPeaks.slice(0, this.trackCount),
      });
      this.peak = 0;
      this.trackPeaks.fill(0);
    }
    return true;
  }

  private runPlugin(left: Float32Array, right: Float32Array, n: number): void {
    const p = this.plugin as WclapPlugin;
    // A mono output bus shares one buffer, so the plugin would process it twice; give it a copy.
    if (right === left) p.process(left, new Float32Array(n), 0, n);
    else p.process(left, right, 0, n);
    const firstBypass = p.bypassed !== null && !this.bypassSent;
    if (++this.pluginBlocks >= this.wclap.reportBlocks || firstBypass) {
      this.pluginBlocks = 0;
      this.bypassSent = p.bypassed !== null;
      this.send({ type: 'wclap-stats', ...p.stats() });
    }
  }

  /** Renders `count` frames of an engine and adds them to the output from `start`. */
  private mix(engine: WasmEngine, left: Float32Array, right: Float32Array, start: number, count: number, measure = false): void {
    for (let off = 0; off < count; off += BLOCK_FRAMES) {
      const [l, r] = engine.process(Math.min(BLOCK_FRAMES, count - off));
      if (measure) this.trackCount = Math.max(this.trackCount, engine.mergeTrackPeaks(this.trackPeaks));
      for (let i = 0; i < l.length; i++) {
        left[start + off + i] = (left[start + off + i] ?? 0) + (l[i] ?? 0);
        right[start + off + i] = (right[start + off + i] ?? 0) + (r[i] ?? 0);
      }
    }
  }
}

registerProcessor('composer-engine', ComposerProcessor);
