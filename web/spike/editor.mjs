// JS wrapper over the ed_* exports of composer_wasm.wasm. No dependencies; works in node, on
// the main thread and in workers. It shares one module instance with WasmEngine if you want.
// Replies are the daemon's JSON-RPC result/error shapes minus the envelope.

const enc = new TextEncoder();
const dec = new TextDecoder();

export class WasmEditor {
  /** @param {WebAssembly.Instance} instance @param {string} name project name */
  constructor(instance, name = 'untitled') {
    this.x = instance.exports;
    const bytes = enc.encode(name);
    const ptr = this.x.cw_alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer).set(bytes, ptr);
    this.handle = this.x.ed_new(ptr, bytes.length);
    this.x.cw_dealloc(ptr, bytes.length);
    if (!this.handle) throw new Error('composer-wasm: invalid project name');
  }

  static fromBytes(wasmBytes, name) {
    const module = new WebAssembly.Module(wasmBytes);
    return new WasmEditor(new WebAssembly.Instance(module, {}), name);
  }

  // Copies the handle's result buffer out before the next call can invalidate it.
  #take(ptr) {
    if (!ptr) throw new Error('composer-wasm: editor call failed');
    const len = this.x.ed_result_len(this.handle);
    return new Uint8Array(this.x.memory.buffer, ptr, len).slice();
  }

  #json(fn, value) {
    const bytes = enc.encode(JSON.stringify(value));
    const ptr = this.x.cw_alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer).set(bytes, ptr);
    const out = fn.call(this.x, this.handle, ptr, bytes.length);
    this.x.cw_dealloc(ptr, bytes.length);
    return JSON.parse(dec.decode(this.#take(out)));
  }

  #plain(fn) {
    return JSON.parse(dec.decode(this.#take(fn.call(this.x, this.handle))));
  }

  /** One command object; returns {result} or {error:{data:{code}}}. */
  apply(command) {
    return this.#json(this.x.ed_apply_json, command);
  }

  /** Atomic list as one undo step. */
  group(label, commands) {
    return this.#json(this.x.ed_group_json, { label, commands });
  }

  undo() {
    return this.#plain(this.x.ed_undo);
  }

  redo() {
    return this.#plain(this.x.ed_redo);
  }

  /** {result:{state_hash, project}}. */
  state() {
    return this.#plain(this.x.ed_state_json);
  }

  describe() {
    return this.#plain(this.x.ed_describe_json);
  }

  /** Replaces the project; history is cleared. */
  load(project) {
    return this.#json(this.x.ed_load_json, project);
  }

  /** State hash as 16 hex digits, the same text the daemon sends. */
  get stateHash() {
    // The u64 comes back as a signed BigInt.
    return BigInt.asUintN(64, this.x.ed_state_hash(this.handle)).toString(16).padStart(16, '0');
  }

  /** Engine blob v1 bytes (a copy) for WasmEngine.load; throws for a zero sample rate. */
  engineBlob(sampleRate) {
    return this.#take(this.x.ed_engine_blob(this.handle, sampleRate));
  }

  free() {
    if (this.handle) this.x.ed_free(this.handle);
    this.handle = 0;
  }
}
