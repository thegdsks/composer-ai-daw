// Our own host for one web plugin (a CLAP module compiled to wasm, 32-bit). It runs wherever wasm runs:
// the audio worklet in the app, node in tests. The plugin sees only its own memory plus a WASI shim
// that implements almost nothing, so it cannot reach the network, the page or the clock.
import { EventQueue } from './events';
import { hostFunction } from './hostFn';
import { bytesToText, inspectPlugin, textToBytes, WASI_NAMESPACE, WclapError, type Inspection, type WclapLimits } from './inspect';

/** Frames per plugin call, the Web Audio render quantum. */
export const PLUGIN_BLOCK = 128;
const CHANNELS = 2;
const PARAM_VALUE_EVENT = 5;
const EVENT_SIZE = 48;
const PARAM_INFO_SIZE = 1320;
const PROCESS_ERROR = 0;
const WASI_NOT_SUPPORTED = 52;
const NAME_MAX = 256;

export interface HostOptions {
  sampleRate: number;
  limits: WclapLimits;
  maxEvents: number;
  /** Share of a block's real duration the plugin may use before the block counts as an overrun. */
  budgetShare: number;
  overrunLimit: number;
  /** Milliseconds clock; the worklet has only a coarse one. */
  now(): number;
}

export interface ParamInfo {
  id: number;
  name: string;
  min: number;
  max: number;
  def: number;
}

export interface PluginStats {
  /** Highest absolute sample going into and out of the plugin since the last read. */
  inPeak: number;
  outPeak: number;
  blocks: number;
  avgBlockUs: number;
  maxBlockUs: number;
  overruns: number;
  droppedEvents: number;
  /** Why the plugin is bypassed, or null while it runs. */
  bypassed: string | null;
}

type Fn = (...args: number[]) => number | void;

export class WclapPlugin {
  readonly inspection: Inspection;
  readonly id: string;
  readonly name: string;
  readonly params: ParamInfo[];
  /** Frames of delay the plugin declares (clap.latency); the inline host adds none of its own. */
  readonly latencyFrames: number;

  private readonly mem: WebAssembly.Memory;
  private readonly table: WebAssembly.Table;
  private readonly fns = new Map<number, Fn>();
  private readonly queue: EventQueue;
  private view: DataView | null = null;
  private f32: Float32Array | null = null;
  private seenBuffer: ArrayBufferLike | null = null;
  private readonly exports: WebAssembly.Exports;
  private readonly plugin: number;
  private readonly entry: number;
  private readonly paramsExt: number;
  private readonly inCh: number[] = [];
  private readonly outCh: number[] = [];
  private readonly proc: number;
  private readonly events: number;
  private evCount = 0;
  private steady = 0;
  private dead: string | null = null;
  private overrunRun = 0;
  private s = { inPeak: 0, outPeak: 0, blocks: 0, totalMs: 0, maxMs: 0, overruns: 0 };

  private constructor(
    instance: WebAssembly.Instance,
    mem: WebAssembly.Memory,
    inspection: Inspection,
    private readonly o: HostOptions,
  ) {
    this.inspection = inspection;
    this.exports = instance.exports;
    this.mem = mem;
    this.table = Object.values(instance.exports).find((e): e is WebAssembly.Table => e instanceof WebAssembly.Table) as WebAssembly.Table;
    this.queue = new EventQueue(o.maxEvents);
    const init = this.exports._initialize;
    if (typeof init === 'function') init();
    this.entry = Number((this.exports.clap_entry as WebAssembly.Global).value);

    const path = this.cstr0('/');
    if (!this.call(this.u32(this.entry + 12), path)) throw new WclapError('exports', 'plugin entry init failed');
    const factory = this.call(this.u32(this.entry + 20), this.cstr0('clap.plugin-factory'));
    if (!factory) throw new WclapError('exports', 'plugin has no plugin factory');
    if (this.call(this.u32(factory), factory) < 1) throw new WclapError('exports', 'plugin factory lists no plugins');
    const desc = this.call(this.u32(factory + 4), factory, 0);
    if (!desc) throw new WclapError('exports', 'plugin factory gave no descriptor');
    this.id = this.readCstr(this.u32(desc + 12));
    this.name = this.readCstr(this.u32(desc + 16));

    const host = this.buildHost();
    this.plugin = this.call(this.u32(factory + 8), factory, host, this.cstr0(this.id));
    if (!this.plugin) throw new WclapError('exports', 'plugin factory could not create the plugin');
    if (!this.call(this.u32(this.plugin + 8), this.plugin)) throw new WclapError('exports', 'plugin init failed');
    if (!this.call(this.u32(this.plugin + 16), this.plugin, o.sampleRate, 1, PLUGIN_BLOCK)) {
      throw new WclapError('exports', 'plugin refused to activate');
    }

    this.proc = this.alloc(40);
    this.events = this.alloc(EVENT_SIZE * o.maxEvents);
    this.buildProcess();
    this.paramsExt = this.extension('clap.params');
    this.params = this.readParams();
    const lat = this.extension('clap.latency');
    this.latencyFrames = lat ? this.call(this.u32(lat), this.plugin) : 0;
    if (!this.call(this.u32(this.plugin + 24), this.plugin)) throw new WclapError('exports', 'plugin would not start processing');
  }

  /** Validates, instantiates and starts a plugin from the bytes of a single .wclap file. Throws WclapError. */
  static load(bytes: Uint8Array, o: HostOptions): WclapPlugin {
    let module: WebAssembly.Module;
    try {
      module = new WebAssembly.Module(bytes as BufferSource);
    } catch (e) {
      throw new WclapError('bad_module', e instanceof Error ? e.message : 'not a valid wasm module');
    }
    const inspection = inspectPlugin(bytes, module, o.limits);
    const ceiling = o.limits.maxMemoryPages;
    let memory: WebAssembly.Memory | null = null;
    const imports: WebAssembly.Imports = {};
    for (const i of WebAssembly.Module.imports(module)) {
      const ns = (imports[i.module] ??= {});
      if (i.kind === 'memory') {
        const m = inspection.memory;
        memory = new WebAssembly.Memory({ initial: m.minPages, maximum: Math.min(m.maxPages ?? ceiling, ceiling), shared: true });
        ns[i.name] = memory;
      } else if (i.module === WASI_NAMESPACE) {
        ns[i.name] = wasiStub(i.name, () => memory);
      }
    }
    let instance: WebAssembly.Instance;
    try {
      instance = new WebAssembly.Instance(module, imports);
    } catch (e) {
      throw new WclapError('bad_module', e instanceof Error ? e.message : 'plugin could not start');
    }
    const own = Object.values(instance.exports).find((e): e is WebAssembly.Memory => e instanceof WebAssembly.Memory);
    const mem = memory ?? own;
    if (!mem) throw new WclapError('memory', 'plugin has no memory');
    if (mem.buffer.byteLength > ceiling * 65536) throw new WclapError('memory', 'plugin memory is over the ceiling');
    try {
      return new WclapPlugin(instance, mem, inspection, o);
    } catch (e) {
      if (e instanceof WclapError) throw e;
      throw new WclapError('bad_module', e instanceof Error ? e.message : 'plugin failed to start');
    }
  }

  get bypassed(): string | null {
    return this.dead;
  }

  /** Stops the plugin for good (kill switch, overrun or fault); audio then passes through untouched. */
  kill(reason: string): void {
    this.dead ??= reason;
  }

  /** Queues a parameter change for the next block, `time` frames into it. */
  setParam(id: number, value: number, time = 0): boolean {
    return this.queue.push(time, id, value);
  }

  /** Current value as the plugin reports it (after the last block), or null for an unknown parameter. */
  getParam(id: number): number | null {
    if (!this.paramsExt) return null;
    const out = this.alloc(8);
    const ok = this.call(this.u32(this.paramsExt + 8), this.plugin, id, out);
    return ok ? this.dv().getFloat64(out, true) : null;
  }

  /** Plugin state blob for the project file. Not in the spike: the clap.state stream bridge comes next. */
  readonly stateSupported = false;

  stats(reset = true): PluginStats {
    const s = this.s;
    const out: PluginStats = {
      inPeak: s.inPeak,
      outPeak: s.outPeak,
      blocks: s.blocks,
      avgBlockUs: s.blocks ? (s.totalMs * 1000) / s.blocks : 0,
      maxBlockUs: s.maxMs * 1000,
      overruns: s.overruns,
      droppedEvents: this.queue.dropped,
      bypassed: this.dead,
    };
    if (reset) {
      s.inPeak = 0;
      s.outPeak = 0;
    }
    return out;
  }

  /** Runs the plugin over `n` frames of the two buffers in place, starting at `from`. */
  process(left: Float32Array, right: Float32Array, from: number, n: number): void {
    for (let off = 0; off < n && !this.dead; off += PLUGIN_BLOCK) {
      this.block(left, right, from + off, Math.min(PLUGIN_BLOCK, n - off));
    }
  }

  destroy(): void {
    try {
      this.call(this.u32(this.plugin + 28), this.plugin);
      this.call(this.u32(this.plugin + 20), this.plugin);
      this.call(this.u32(this.plugin + 12), this.plugin);
      this.call(this.u32(this.entry + 16));
    } catch {
      // a plugin that traps while closing is already unloaded as far as we are concerned
    }
    this.dead ??= 'unloaded';
  }

  private block(left: Float32Array, right: Float32Array, at: number, n: number): void {
    const t0 = this.o.now();
    try {
      const f = this.floats();
      const l = this.inCh[0] as number;
      const r = this.inCh[1] as number;
      f.set(left.subarray(at, at + n), l >> 2);
      f.set(right.subarray(at, at + n), r >> 2);
      let inPeak = 0;
      for (let i = 0; i < n; i++) inPeak = Math.max(inPeak, Math.abs(left[at + i] ?? 0), Math.abs(right[at + i] ?? 0));
      this.writeEvents(this.queue.take(n));
      const dv = this.dv();
      dv.setUint32(this.proc + 8, n, true);
      dv.setBigInt64(this.proc, BigInt(this.steady), true);
      this.steady += n;
      const status = this.call(this.u32(this.plugin + 36), this.plugin, this.proc);
      if (status === PROCESS_ERROR) {
        this.kill('the plugin reported a process error');
        return;
      }
      const g = this.floats();
      const ol = this.outCh[0] as number;
      const or = this.outCh[1] as number;
      left.set(g.subarray(ol >> 2, (ol >> 2) + n), at);
      right.set(g.subarray(or >> 2, (or >> 2) + n), at);
      let outPeak = 0;
      for (let i = 0; i < n; i++) outPeak = Math.max(outPeak, Math.abs(left[at + i] ?? 0), Math.abs(right[at + i] ?? 0));
      this.s.inPeak = Math.max(this.s.inPeak, inPeak);
      this.s.outPeak = Math.max(this.s.outPeak, outPeak);
    } catch (e) {
      this.kill(e instanceof Error ? `the plugin faulted: ${e.message}` : 'the plugin faulted');
      return;
    }
    const ms = this.o.now() - t0;
    this.s.blocks++;
    this.s.totalMs += ms;
    this.s.maxMs = Math.max(this.s.maxMs, ms);
    if (ms > (n / this.o.sampleRate) * 1000 * this.o.budgetShare) {
      this.s.overruns++;
      if (++this.overrunRun >= this.o.overrunLimit) this.kill('the plugin ran over its time budget');
    } else this.overrunRun = 0;
  }

  private writeEvents(list: { time: number; id: number; value: number }[]): void {
    const dv = this.dv();
    this.evCount = list.length;
    list.forEach((e, i) => {
      const p = this.events + i * EVENT_SIZE;
      dv.setUint32(p, EVENT_SIZE, true);
      dv.setUint32(p + 4, e.time, true);
      dv.setUint16(p + 8, 0, true);
      dv.setUint16(p + 10, PARAM_VALUE_EVENT, true);
      dv.setUint32(p + 12, 0, true);
      dv.setUint32(p + 16, e.id, true);
      dv.setUint32(p + 20, 0, true);
      dv.setInt32(p + 24, -1, true);
      dv.setInt16(p + 28, -1, true);
      dv.setInt16(p + 30, -1, true);
      dv.setInt16(p + 32, -1, true);
      dv.setFloat64(p + 40, e.value, true);
    });
  }

  private buildHost(): number {
    const slot = (sig: string, fn: Fn): number => {
      let idx: number;
      try {
        idx = this.table.grow(1);
      } catch {
        throw new WclapError('exports', 'the plugin function table cannot grow');
      }
      this.table.set(idx, hostFunction(sig, fn));
      return idx;
    };
    const host = this.alloc(48);
    const dv = this.dv();
    dv.setUint32(host, 1, true);
    dv.setUint32(host + 4, 2, true);
    dv.setUint32(host + 8, 7, true);
    dv.setUint32(host + 16, this.cstr0('Composer'), true);
    dv.setUint32(host + 20, this.cstr0('Composer'), true);
    dv.setUint32(host + 28, this.cstr0('0'), true);
    // No host extensions are offered, so a plugin cannot ask for logging, threads or timers.
    dv.setUint32(host + 32, slot('ii:i', () => 0), true);
    for (const at of [36, 40, 44]) dv.setUint32(host + at, slot('i', () => undefined), true);
    this.inEvents = this.alloc(12);
    const ev = this.dv();
    ev.setUint32(this.inEvents + 4, slot('i:i', () => this.evCount), true);
    ev.setUint32(this.inEvents + 8, slot('ii:i', (_l, i) => (i < this.evCount ? this.events + i * EVENT_SIZE : 0)), true);
    this.outEvents = this.alloc(8);
    this.dv().setUint32(this.outEvents + 4, slot('ii:i', () => 1), true);
    return host;
  }

  private inEvents = 0;
  private outEvents = 0;

  private buildProcess(): void {
    const frames = PLUGIN_BLOCK * 4;
    const chans = (): { ptrs: number; buf: number } => {
      const ptrs = this.alloc(CHANNELS * 4);
      const buf = this.alloc(24);
      const dv = this.dv();
      dv.setUint32(buf, ptrs, true);
      dv.setUint32(buf + 8, CHANNELS, true);
      return { ptrs, buf };
    };
    const i = chans();
    const o = chans();
    for (let c = 0; c < CHANNELS; c++) {
      const a = this.alloc(frames);
      const b = this.alloc(frames);
      this.inCh.push(a);
      this.outCh.push(b);
      this.dv().setUint32(i.ptrs + c * 4, a, true);
      this.dv().setUint32(o.ptrs + c * 4, b, true);
    }
    const dv = this.dv();
    dv.setUint32(this.proc + 16, i.buf, true);
    dv.setUint32(this.proc + 20, o.buf, true);
    dv.setUint32(this.proc + 24, 1, true);
    dv.setUint32(this.proc + 28, 1, true);
    dv.setUint32(this.proc + 32, this.inEvents, true);
    dv.setUint32(this.proc + 36, this.outEvents, true);
  }

  private readParams(): ParamInfo[] {
    if (!this.paramsExt) return [];
    const count = this.call(this.u32(this.paramsExt), this.plugin);
    const buf = this.alloc(PARAM_INFO_SIZE);
    const out: ParamInfo[] = [];
    for (let i = 0; i < count; i++) {
      if (!this.call(this.u32(this.paramsExt + 4), this.plugin, i, buf)) continue;
      const dv = this.dv();
      out.push({
        id: dv.getUint32(buf, true),
        name: this.readCstr(buf + 12),
        min: dv.getFloat64(buf + 1296, true),
        max: dv.getFloat64(buf + 1304, true),
        def: dv.getFloat64(buf + 1312, true),
      });
    }
    return out;
  }

  private extension(id: string): number {
    return this.call(this.u32(this.plugin + 40), this.plugin, this.cstr0(id));
  }

  /** The plugin memory may grow, which detaches old views, so views are rebuilt when the buffer changes. */
  private sync(): void {
    if (this.seenBuffer === this.mem.buffer) return;
    this.seenBuffer = this.mem.buffer;
    this.view = new DataView(this.mem.buffer);
    this.f32 = new Float32Array(this.mem.buffer);
  }

  private dv(): DataView {
    this.sync();
    return this.view as DataView;
  }

  private floats(): Float32Array {
    this.sync();
    return this.f32 as Float32Array;
  }

  private u32(p: number): number {
    return this.dv().getUint32(p, true);
  }

  private fn(idx: number): Fn {
    let f = this.fns.get(idx);
    if (!f) {
      f = this.table.get(idx) as Fn | null as Fn;
      if (!f) throw new WclapError('exports', 'the plugin called a null function pointer');
      this.fns.set(idx, f);
    }
    return f;
  }

  private call(idx: number, ...args: number[]): number {
    return Number(this.fn(idx)(...args));
  }

  private alloc(size: number): number {
    const p = Number((this.exports.malloc as Fn)(size)) >>> 0;
    if (!p) throw new WclapError('memory', 'plugin allocator returned null');
    new Uint8Array(this.mem.buffer, p, size).fill(0);
    return p;
  }

  private cstr0(s: string): number {
    const bytes = textToBytes(s);
    const p = this.alloc(bytes.length + 1);
    new Uint8Array(this.mem.buffer, p, bytes.length).set(bytes);
    return p;
  }

  private readCstr(p: number): string {
    const b = new Uint8Array(this.mem.buffer, p, Math.min(NAME_MAX, this.mem.buffer.byteLength - p));
    const end = b.indexOf(0);
    return bytesToText(b.subarray(0, end < 0 ? b.length : end));
  }
}

function wasiStub(name: string, memory: () => WebAssembly.Memory | null): Fn {
  if (name === 'random_get') {
    return (ptr = 0, len = 0) => {
      const m = memory();
      if (m) for (let i = 0; i < len; i++) new Uint8Array(m.buffer)[ptr + i] = Math.floor(Math.random() * 256);
      return 0;
    };
  }
  if (name === 'proc_exit') {
    return () => {
      throw new WclapError('unsupported', 'the plugin tried to exit');
    };
  }
  return () => WASI_NOT_SUPPORTED;
}
