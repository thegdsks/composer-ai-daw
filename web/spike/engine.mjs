// Tiny loader for composer_wasm.wasm. Runs in node and in an AudioWorkletGlobalScope.
// The module has no imports; every call goes through the C ABI (cw_*).

export class WasmEngine {
  constructor(instance) {
    this.x = instance.exports;
    this.handle = 0;
  }

  static fromBytes(bytes) {
    const module = new WebAssembly.Module(bytes);
    return new WasmEngine(new WebAssembly.Instance(module, {}));
  }

  // Typed views must be rebuilt after every call that may grow memory.
  #u8() {
    return new Uint8Array(this.x.memory.buffer);
  }

  /** Builds the demo blob inside the module and returns a copy of its bytes. */
  demoBlob(sampleRate) {
    const lenPtr = this.x.cw_alloc(4);
    const ptr = this.x.cw_demo_blob(sampleRate, lenPtr);
    const len = new DataView(this.x.memory.buffer).getUint32(lenPtr, true);
    const blob = this.#u8().slice(ptr, ptr + len);
    this.x.cw_dealloc(ptr, len);
    this.x.cw_dealloc(lenPtr, 4);
    return blob;
  }

  /** Creates the engine from a blob (see the composer-wasm rustdoc). Throws on a bad blob. */
  load(blob) {
    this.free();
    const ptr = this.x.cw_alloc(blob.length);
    this.#u8().set(blob, ptr);
    const h = this.x.cw_create(ptr, blob.length);
    this.x.cw_dealloc(ptr, blob.length);
    if (!h) throw new Error('composer-wasm: invalid blob');
    this.handle = h;
  }

  free() {
    if (this.handle) this.x.cw_free(this.handle);
    this.handle = 0;
  }

  /** Renders up to 128 frames; returns [left, right] views valid until the next call. */
  process(frames) {
    const n = this.x.cw_process(this.handle, frames);
    const buf = this.x.memory.buffer;
    const l = new Float32Array(buf, this.x.cw_left(this.handle), n);
    const r = new Float32Array(buf, this.x.cw_right(this.handle), n);
    return [l, r];
  }

  get position() {
    return Number(this.x.cw_position(this.handle));
  }

  get length() {
    return Number(this.x.cw_length(this.handle));
  }

  get clipped() {
    return Number(this.x.cw_clipped(this.handle));
  }

  note(track, pitch, velocity, on) {
    return this.x.cw_note(this.handle, track, pitch, velocity, on ? 1 : 0) === 1;
  }
}

/** FNV-1a 64 over the little endian bytes of f32 samples, same as the Rust tests. */
export function fnv1a64(samples) {
  const bytes = new Uint8Array(samples.buffer, samples.byteOffset, samples.byteLength);
  let h = 0xcbf29ce484222325n;
  for (const b of bytes) {
    h ^= BigInt(b);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return h;
}
