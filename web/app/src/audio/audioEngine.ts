// Main thread side of playback: owns the AudioContext and the worklet node. The context is
// created on the first user gesture (browsers refuse to start audio before one).
import { config } from '../config';
import type { Reply } from '../backend/types';
import { locate, type CapturePos, type ClockAnchor } from '../input/recorder';
import { countInClicks, secondsToTicks, ticksToSeconds } from '../time';
import { buildPlan, planKey, type TransportInputs } from './plan';
import type { CountIn, FromWorklet, PluginInfo, ToWorklet, WorkletOptions } from './protocol';
import { inspectPlugin, WclapError } from './wclap/inspect';
import { previewNotes } from './preview';
import workletUrl from './worklet.ts?worker&url';

export type AudioState = 'off' | 'starting' | 'running' | 'suspended' | 'error';
export type Transport = 'stopped' | 'playing' | 'paused';

export interface AudioStatus {
  state: AudioState;
  transport: Transport;
  sampleRate: number;
  /** Playhead in sample frames of the compiled blob. */
  positionFrames: number;
  lengthFrames: number;
  /** The count in is playing and the song has not started moving. */
  counting: boolean;
  /** Seeks finished, loop wraps and clicks started in the worklet. */
  seeks: number;
  loopWraps: number;
  clicks: number;
  clickPeak: number;
  /** Highest sample in the last report and the highest since the context started. */
  peak: number;
  maxPeak: number;
  /** Measured peak of each track in the last report (linear, post fader and pan); empty before the first report from a worker that sends them. */
  trackPeaks: Float32Array;
  clippedFrames: number;
  blobBytes: number;
  reloads: number;
  /** Sound browser auditions started, and the sound of the last one. */
  previews: number;
  previewSound: string | null;
  reports: number;
  error: string | null;
  plugin: PluginStatus | null;
  /** Why the last plugin load failed, or null. */
  pluginError: string | null;
}

/** The web plugin on the master output. Its audio is not bit exact, so renders that include it are flagged. */
export interface PluginStatus extends PluginInfo {
  nonDeterministic: true;
  bytes: number;
  inPeak: number;
  outPeak: number;
  blocks: number;
  avgBlockUs: number;
  maxBlockUs: number;
  overruns: number;
  droppedEvents: number;
  bypassed: string | null;
  /** Extra delay the host adds on top of the plugin's own: none, the plugin runs inline in the audio callback. */
  hostLatencyFrames: 0;
  /** Milliseconds from handing over the file to the plugin running in the worklet. */
  loadMs: number;
}

export type PluginLoad = { ok: true; info: PluginInfo } | { ok: false; code: string; message: string };

export type BlobSource = (sampleRate: number) => Promise<Reply<Uint8Array>>;
/** Compiles the small blob that carries one sound for the audition engine. */
export type PreviewSource = (sound: string, sampleRate: number) => Uint8Array | Promise<Uint8Array>;

export class AudioEngine {
  readonly status: AudioStatus = {
    state: 'off',
    transport: 'stopped',
    sampleRate: 0,
    positionFrames: 0,
    lengthFrames: 0,
    counting: false,
    seeks: 0,
    loopWraps: 0,
    clicks: 0,
    clickPeak: 0,
    peak: 0,
    maxPeak: 0,
    trackPeaks: new Float32Array(0),
    clippedFrames: 0,
    blobBytes: 0,
    reloads: 0,
    previews: 0,
    previewSound: null,
    reports: 0,
    error: null,
    plugin: null,
    pluginError: null,
  };
  private ctx: AudioContext | null = null;
  private node: AudioWorkletNode | null = null;
  private starting: Promise<boolean> | null = null;
  private reloadTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly listeners = new Set<() => void>();
  private inputs: TransportInputs | null = null;
  private sentPlan = '';
  /** Where Play last started, the place a second Stop returns to. */
  private lastStartFrame = 0;
  private seeksAsked = 0;
  /** Plays again from the start when the song ends, until a loop range exists in the transport. */
  private repeat = false;
  /** Last playhead report paired with the audio clock; recording reads positions from it. */
  private anchor: { clock: number; frames: number; pass: number } | null = null;

  constructor(
    private readonly wasm: ArrayBuffer,
    private readonly blobSource: BlobSource,
    private readonly previewSource: PreviewSource | null = null,
  ) {}

  private previewLoaded: string | null = null;
  private previewTimer: ReturnType<typeof setTimeout> | null = null;
  private previewHeld: number[] = [];

  /** Fires on coarse changes (state, transport); the playhead is read from `status`. */
  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  private emit(): void {
    for (const l of this.listeners) l();
  }

  private post(m: ToWorklet): void {
    this.node?.port.postMessage(m);
  }

  /** Output delay in milliseconds as the browser reports it, or null before audio starts. */
  get latencyMs(): number | null {
    if (!this.ctx) return null;
    return Math.round((this.ctx.baseLatency + (this.ctx.outputLatency || 0)) * 1000);
  }

  /** Creates the context and worklet once; call from a user gesture. */
  ensure(): Promise<boolean> {
    if (this.node) return Promise.resolve(true);
    this.starting ??= this.start();
    return this.starting;
  }

  private async start(): Promise<boolean> {
    this.status.state = 'starting';
    this.emit();
    try {
      const ctx = new AudioContext({ latencyHint: 'interactive' });
      this.ctx = ctx;
      await ctx.audioWorklet.addModule(workletUrl);
      const options: WorkletOptions = {
        wasm: this.wasm,
        positionReportFrames: config.positionReportFrames,
        clickMs: config.clickMs,
        clickGain: config.clickGain,
        clickHz: config.clickHz,
        clickAccentHz: config.clickAccentHz,
        seekBudgetFrames: config.seekBudgetFrames,
        wclap: {
          maxFileBytes: config.wclapMaxFileBytes,
          maxMemoryPages: config.wclapMaxMemoryPages,
          maxEvents: config.wclapMaxEvents,
          budgetShare: config.wclapBudgetShare,
          overrunLimit: config.wclapOverrunLimit,
          reportBlocks: config.wclapReportBlocks,
        },
      };
      const node = new AudioWorkletNode(ctx, 'composer-engine', {
        numberOfInputs: 0,
        outputChannelCount: [2],
        processorOptions: options,
      });
      node.port.onmessage = (e: MessageEvent<FromWorklet>) => this.onReport(e.data);
      node.onprocessorerror = () => this.fail('audio processor error');
      node.connect(ctx.destination);
      this.node = node;
      ctx.onstatechange = () => this.syncState();
      await ctx.resume();
      this.status.sampleRate = ctx.sampleRate;
      this.syncState();
      await this.reload(false);
      this.pushPlan();
      return true;
    } catch (e) {
      this.fail(e instanceof Error ? e.message : 'audio failed to start');
      this.starting = null;
      return false;
    }
  }

  private fail(message: string): void {
    this.status.state = 'error';
    this.status.error = message;
    this.emit();
  }

  private syncState(): void {
    if (!this.ctx || this.status.state === 'error') return;
    this.status.state = this.ctx.state === 'running' ? 'running' : 'suspended';
    this.emit();
  }

  private onReport(m: FromWorklet): void {
    const s = this.status;
    if (m.type === 'pos') {
      // A report from before the last seek landed must not drag the playhead back.
      if (m.seeks >= this.seeksAsked) s.positionFrames = m.frames;
      s.counting = m.counting;
      this.anchor = m.playing && !m.counting && m.seeks >= this.seeksAsked ? { clock: m.at, frames: m.frames, pass: m.wraps } : null;
      s.seeks = m.seeks;
      s.loopWraps = m.wraps;
      s.clicks = m.clicks;
      s.clickPeak = m.clickPeak;
      s.lengthFrames = m.lengthFrames;
      s.peak = m.peak;
      s.maxPeak = Math.max(s.maxPeak, m.peak);
      s.clippedFrames = m.clipped;
      s.trackPeaks = m.trackPeaks ?? new Float32Array(0);
      s.reports++;
      const next: Transport = m.playing ? 'playing' : s.transport === 'playing' ? 'paused' : s.transport;
      if (next !== s.transport) {
        s.transport = next;
        this.emit();
      }
    } else if (m.type === 'wclap-loaded' || m.type === 'wclap-failed') {
      const done = this.pluginWaiters.get(m.id);
      this.pluginWaiters.delete(m.id);
      done?.(m.type === 'wclap-loaded' ? { ok: true, info: m.info } : { ok: false, code: m.code, message: m.message });
    } else if (m.type === 'wclap-stats') {
      if (s.plugin) {
        s.plugin.inPeak = m.inPeak;
        s.plugin.outPeak = m.outPeak;
        s.plugin.blocks = m.blocks;
        s.plugin.avgBlockUs = m.avgBlockUs;
        s.plugin.maxBlockUs = m.maxBlockUs;
        s.plugin.overruns = m.overruns;
        s.plugin.droppedEvents = m.droppedEvents;
        s.plugin.bypassed = m.bypassed;
      }
    } else if (m.type === 'ended') {
      s.transport = 'stopped';
      s.counting = false;
      s.positionFrames = 0;
      this.emit();
      if (this.repeat) void this.play();
    } else if (m.type === 'error') {
      this.fail(m.message);
    }
  }

  private pluginSeq = 0;
  private readonly pluginWaiters = new Map<number, (r: PluginLoad) => void>();

  /** Loads one .wclap file as an insert on the master output. Checks on the page first, then again in the worklet. */
  async loadPlugin(file: File): Promise<PluginLoad> {
    const fail = (code: string, message: string): PluginLoad => {
      this.status.pluginError = message;
      return { ok: false, code, message };
    };
    if (file.size > config.wclapMaxFileBytes) return fail('too_large', `The plugin file is larger than the ${config.wclapMaxFileBytes} byte limit.`);
    const t0 = performance.now();
    const bytes = new Uint8Array(await file.arrayBuffer());
    try {
      inspectPlugin(bytes, await WebAssembly.compile(bytes as BufferSource), {
        maxFileBytes: config.wclapMaxFileBytes,
        maxMemoryPages: config.wclapMaxMemoryPages,
      });
    } catch (e) {
      return e instanceof WclapError ? fail(e.code, e.message) : fail('bad_module', 'That file is not a web plugin.');
    }
    if (!(await this.ensure()) || !this.node) return fail('audio', 'Audio did not start.');
    await this.ctx?.resume();
    const id = ++this.pluginSeq;
    const answer = new Promise<PluginLoad>((resolve) => {
      this.pluginWaiters.set(id, resolve);
      setTimeout(() => {
        if (this.pluginWaiters.delete(id)) resolve(fail('timeout', 'The plugin did not start in time.'));
      }, config.wclapLoadTimeoutMs);
    });
    const size = bytes.length;
    this.node.port.postMessage({ type: 'wclap-load', bytes, id } satisfies ToWorklet, [bytes.buffer]);
    const r = await answer;
    if (!r.ok) this.status.pluginError = r.message;
    if (r.ok) {
      this.status.pluginError = null;
      this.status.plugin = {
        ...r.info,
        nonDeterministic: true,
        bytes: size,
        inPeak: 0,
        outPeak: 0,
        blocks: 0,
        avgBlockUs: 0,
        maxBlockUs: 0,
        overruns: 0,
        droppedEvents: 0,
        bypassed: null,
        hostLatencyFrames: 0,
        loadMs: Math.round(performance.now() - t0),
      };
      this.emit();
    }
    return r;
  }

  /** Sets a plugin parameter; it lands at the start of the next audio block. */
  setPluginParam(param: number, value: number): void {
    if (this.status.plugin) this.post({ type: 'wclap-param', param, value });
  }

  /** Kill switch: removes the plugin; the audio carries on without it. */
  removePlugin(): void {
    this.post({ type: 'wclap-unload' });
    this.status.plugin = null;
    this.emit();
  }

  /** Rebuilds the blob after an edit, debounced; a no-op until audio has started. */
  scheduleReload(): void {
    if (!this.node) return;
    if (this.reloadTimer) clearTimeout(this.reloadTimer);
    this.reloadTimer = setTimeout(() => void this.reload(true), config.blobDebounceMs);
  }

  private async reload(keepPosition: boolean): Promise<void> {
    if (!this.ctx) return;
    const r = await this.blobSource(this.ctx.sampleRate);
    if (!r.ok) {
      this.fail(r.error.message);
      return;
    }
    this.status.blobBytes = r.value.length;
    this.status.reloads++;
    this.post({
      type: 'load',
      blob: r.value,
      keepPosition,
      maxSeekFrames: Math.floor(config.maxSeekSeconds * this.ctx.sampleRate),
    });
  }

  /** Tempo map, loop, metronome and song end; the worklet gets a frame plan whenever they change. */
  setTransportInputs(i: TransportInputs): void {
    this.inputs = i;
    this.pushPlan();
  }

  private pushPlan(): void {
    if (!this.node || !this.ctx || !this.inputs) return;
    const key = planKey(this.inputs);
    if (key === this.sentPlan) return;
    this.sentPlan = key;
    this.post({ type: 'plan', plan: buildPlan(this.inputs, this.ctx.sampleRate) });
  }

  /** Playhead as a tick of the current tempo map. */
  get playTick(): number {
    const s = this.status;
    if (!this.inputs || s.sampleRate <= 0) return 0;
    return secondsToTicks(this.inputs.map, s.positionFrames / s.sampleRate);
  }

  /**
   * Where the song is right now on the audio clock, for timing live notes. The press is placed
   * where the player heard the song, so the output delay is taken off. Null until a clean
   * position report has arrived (never during the count in). `afterStop` keeps answering once paused.
   */
  capturePosition(afterStop = false): CapturePos | null {
    const a = this.anchor;
    const i = this.inputs;
    if (!a || !i || !this.ctx || this.status.sampleRate <= 0) return null;
    if (!afterStop && this.status.transport !== 'playing') return null;
    const delay = ((this.ctx.outputLatency || 0) + this.ctx.baseLatency) * config.recordLatencyShare;
    const anchor: ClockAnchor = { clock: a.clock, tick: secondsToTicks(i.map, a.frames / this.status.sampleRate), pass: a.pass };
    const l = i.prefs.loop;
    return locate(i.map, anchor, this.ctx.currentTime - delay, { on: l.on, start: l.start, end: l.end });
  }

  private countIn(force: boolean): CountIn | null {
    const i = this.inputs;
    if (!i || !this.ctx) return null;
    const bars = i.prefs.countInBars;
    if (bars <= 0 || !(force || i.prefs.metronome.on)) return null;
    const c = countInClicks(i.map, this.playTick, bars, this.ctx.sampleRate);
    return {
      frames: Float64Array.from(c.clicks, (k) => k.frame),
      accents: Uint8Array.from(c.clicks, (k) => (k.accent ? 1 : 0)),
      totalFrames: c.totalFrames,
    };
  }

  /** `forceCountIn` is for Record, which always counts in; Play counts in only with the metronome on. */
  async play(forceCountIn = false): Promise<void> {
    if (!(await this.ensure())) return;
    await this.ctx?.resume();
    if (this.reloadTimer) {
      clearTimeout(this.reloadTimer);
      this.reloadTimer = null;
      await this.reload(true);
    }
    this.pushPlan();
    this.lastStartFrame = this.status.positionFrames;
    const countIn = this.status.transport === 'playing' ? null : this.countIn(forceCountIn);
    if (countIn || this.status.transport !== 'playing') this.anchor = null;
    this.status.transport = 'playing';
    this.status.counting = countIn !== null;
    this.post({ type: 'play', countIn });
    this.emit();
  }

  setRepeat(on: boolean): void {
    this.repeat = on;
  }

  /** Loads the project as it is now and plays from bar 1; for a piece that just replaced the old one. */
  async playFromStart(repeat: boolean): Promise<void> {
    if (!(await this.ensure())) return;
    await this.ctx?.resume();
    if (this.reloadTimer) {
      clearTimeout(this.reloadTimer);
      this.reloadTimer = null;
    }
    await this.reload(false);
    this.repeat = repeat;
    this.status.transport = 'playing';
    this.post({ type: 'play', countIn: null });
    this.emit();
  }

  pause(): void {
    this.status.transport = 'paused';
    this.status.counting = false;
    this.post({ type: 'pause' });
    this.emit();
  }

  /** The first Stop halts where the playhead is; Stop again goes back to where Play started. */
  stop(): void {
    if (this.status.transport === 'playing') {
      this.pause();
      return;
    }
    this.status.transport = 'stopped';
    this.seekFrames(this.lastStartFrame);
  }

  /** Moves the playhead to a tick; playing carries on from there. */
  async seekTick(tick: number): Promise<void> {
    if (!(await this.ensure()) || !this.inputs || !this.ctx) return;
    const frame = Math.max(0, Math.round(ticksToSeconds(this.inputs.map, Math.max(0, tick)) * this.ctx.sampleRate));
    this.seekFrames(frame);
  }

  private seekFrames(frame: number): void {
    this.seeksAsked++;
    this.anchor = null;
    this.status.positionFrames = frame;
    this.post({ type: 'seek', frame, id: this.seeksAsked });
    this.emit();
  }

  async toggle(): Promise<void> {
    if (this.status.transport === 'playing') this.pause();
    else await this.play();
  }

  /** Live note on or off on a track index; dropped until audio has started. */
  liveNote(track: number, pitch: number, velocity: number, on: boolean): void {
    if (!this.node) {
      if (on) void this.ensure();
      return;
    }
    this.post({ type: 'note', track, pitch, velocity, on });
  }

  private releasePreview(): void {
    if (this.previewTimer) clearTimeout(this.previewTimer);
    this.previewTimer = null;
    for (const pitch of this.previewHeld) this.post({ type: 'preview-note', pitch, velocity: 0, on: false });
    this.previewHeld = [];
  }

  /** Plays a short chord of one sound without touching the project. Returns false when it cannot. */
  async preview(sound: string): Promise<boolean> {
    if (!this.previewSource || !(await this.ensure()) || !this.ctx) return false;
    await this.ctx.resume();
    this.releasePreview();
    try {
      if (this.previewLoaded !== sound) {
        this.post({ type: 'preview-load', blob: await this.previewSource(sound, this.ctx.sampleRate) });
        this.previewLoaded = sound;
      }
    } catch (e) {
      this.previewLoaded = null;
      this.fail(e instanceof Error ? e.message : 'preview failed');
      return false;
    }
    const pitches = previewNotes(config.previewRootPitch);
    for (const pitch of pitches) this.post({ type: 'preview-note', pitch, velocity: config.previewVelocity, on: true });
    this.previewHeld = pitches;
    this.status.previews++;
    this.status.previewSound = sound;
    this.previewTimer = setTimeout(() => this.releasePreview(), config.previewHoldMs);
    return true;
  }

  dispose(): void {
    this.releasePreview();
    if (this.reloadTimer) clearTimeout(this.reloadTimer);
    this.node?.disconnect();
    void this.ctx?.close();
    this.node = null;
    this.ctx = null;
  }
}
