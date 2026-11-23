// Turns a JS function into a wasm function that can sit in a plugin's function table. A table slot
// only accepts wasm functions, so a two line module imports the JS function and re-exports it.

const TYPE: Record<string, number> = { i: 0x7f, l: 0x7e, f: 0x7d, d: 0x7c };

function section(id: number, body: number[]): number[] {
  return [id, body.length, ...body];
}

function str(s: string): number[] {
  return [s.length, ...Array.from(s, (c) => c.charCodeAt(0))];
}

/** Signature like "ii:i" (params, colon, optional result); letters i l f d are i32 i64 f32 f64. */
export function forwardingModule(sig: string): Uint8Array {
  const [params = '', result = ''] = sig.split(':');
  const p = Array.from(params, (c) => TYPE[c] as number);
  const r = Array.from(result, (c) => TYPE[c] as number);
  const type = section(1, [1, 0x60, p.length, ...p, r.length, ...r]);
  const imports = section(2, [1, ...str('h'), ...str('f'), 0x00, 0x00]);
  const exports = section(7, [1, ...str('f'), 0x00, 0x00]);
  return Uint8Array.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, ...type, ...imports, ...exports]);
}

const cache = new Map<string, WebAssembly.Module>();

/** A wasm function with the given signature that calls `fn`. */
export function hostFunction(sig: string, fn: (...args: number[]) => number | void): WebAssembly.ExportValue {
  let module = cache.get(sig);
  if (!module) {
    module = new WebAssembly.Module(forwardingModule(sig) as BufferSource);
    cache.set(sig, module);
  }
  const instance = new WebAssembly.Instance(module, { h: { f: fn } });
  return instance.exports.f as WebAssembly.ExportValue;
}
