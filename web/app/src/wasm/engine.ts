// Typed loader for the cw_* engine exports. Runs on the main thread, in node and inside an
// AudioWorkletGlobalScope (no DOM or TextEncoder use). The module has no imports.

interface EngineExports {
  memory: WebAssembly.Memory;
  cw_alloc(len: number): number;
  cw_dealloc(ptr: number, len: number): void;
  cw_create(ptr: number, len: number): number;
  cw_free(h: number): void;
  cw_process(h: number, frames: number): number;
  cw_left(h: number): number;
  cw_right(h: number): number;
  cw_position(h: number): bigint;
  cw_length(h: number): bigint;
  cw_clipped(h: number): bigint;
  cw_track_peaks(h: number, out: number, max: number): number;
  cw_note(h: number, track: number, pitch: number, velocity: number, on: number): number;
}

/** Frames per engine call; the engine renders at most this many at once. */
export const BLOCK_FRAMES = 128;

/** Most tracks whose peaks one engine reports (scratch space is reserved at load). */
export const MAX_PEAK_TRACKS = 1024;

export class WasmEngine {
  private readonly x: EngineExports;
  private handle = 0;
  private peakPtr = 0;
  private peakView: Float32Array | null = null;
  private peakBuffer: ArrayBufferLike | null = null;

  constructor(instance: WebAssembly.Instance) {
    this.x = instance.exports as unknown as EngineExports;
  }

  static fromBytes(bytes: BufferSource): WasmEngine {
    return WasmEngine.fromModule(new WebAssembly.Module(bytes));
  }

  static fromModule(module: WebAssembly.Module): WasmEngine {
    return new WasmEngine(new WebAssembly.Instance(module, {}));
  }

  get loaded(): boolean {
    return this.handle !== 0;
  }

  /** Creates the engine from a blob. Throws on a bad blob. */
  load(blob: Uint8Array): void {
    this.free();
    const ptr = this.x.cw_alloc(blob.length);
    new Uint8Array(this.x.memory.buffer).set(blob, ptr);
    const h = this.x.cw_create(ptr, blob.length);
    this.x.cw_dealloc(ptr, blob.length);
    if (!h) throw new Error('composer-wasm: invalid blob');
    this.handle = h;
    if (!this.peakPtr) this.peakPtr = this.x.cw_alloc(MAX_PEAK_TRACKS * 4);
  }

  free(): void {
    if (this.handle) this.x.cw_free(this.handle);
    this.handle = 0;
  }

  /**
   * Max-merges the per track peaks of the last block into `into` (blob track order) and returns
   * the track count. Allocation free once the memory view is cached.
   */
  mergeTrackPeaks(into: Float32Array): number {
    if (!this.handle || !this.peakPtr) return 0;
    const count = this.x.cw_track_peaks(this.handle, this.peakPtr, MAX_PEAK_TRACKS);
    const buf = this.x.memory.buffer;
    if (this.peakBuffer !== buf || !this.peakView) {
      this.peakView = new Float32Array(buf, this.peakPtr, MAX_PEAK_TRACKS);
      this.peakBuffer = buf;
    }
    const n = Math.min(count, MAX_PEAK_TRACKS, into.length);
    for (let i = 0; i < n; i++) {
      const v = this.peakView[i] as number;
      if (v > (into[i] as number)) into[i] = v;
    }
    return Math.min(count, MAX_PEAK_TRACKS);
  }

  /** Renders up to 128 frames; the views are valid until the next call. */
  process(frames: number): [Float32Array, Float32Array] {
    const n = this.x.cw_process(this.handle, frames);
    const buf = this.x.memory.buffer;
    return [
      new Float32Array(buf, this.x.cw_left(this.handle), n),
      new Float32Array(buf, this.x.cw_right(this.handle), n),
    ];
  }

  /** Renders and discards, used to catch up after a reload. */
  skip(frames: number): void {
    let left = frames;
    while (left > 0) {
      const n = Math.min(BLOCK_FRAMES, left);
      this.x.cw_process(this.handle, n);
      left -= n;
    }
  }

  get position(): number {
    return Number(this.x.cw_position(this.handle));
  }

  get length(): number {
    return Number(this.x.cw_length(this.handle));
  }

  get clipped(): number {
    return Number(this.x.cw_clipped(this.handle));
  }

  note(track: number, pitch: number, velocity: number, on: boolean): boolean {
    return this.x.cw_note(this.handle, track, pitch, velocity, on ? 1 : 0) === 1;
  }
}
