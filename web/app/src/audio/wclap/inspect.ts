// Static checks of a web plugin file before anything runs: size, memory declaration, imports
// and exports. Pure functions over bytes and a compiled module, usable on the page and in the worklet.

export interface WclapLimits {
  maxFileBytes: number;
  maxMemoryPages: number;
}

export interface MemoryDecl {
  minPages: number;
  maxPages: number | null;
  shared: boolean;
  /** True when the module imports its memory, false when it declares and exports its own. */
  imported: boolean;
}

export type WclapErrorCode = 'too_large' | 'bad_module' | 'memory' | 'imports' | 'exports' | 'unsupported';

export class WclapError extends Error {
  constructor(
    readonly code: WclapErrorCode,
    message: string,
  ) {
    super(message);
    this.name = 'WclapError';
  }
}

/** The only function imports a plugin may have: a WASI shim that implements almost nothing. */
export const WASI_NAMESPACE = 'wasi_snapshot_preview1';

/** The worklet scope has no TextEncoder or TextDecoder, so text is handled byte per char (names are ASCII in practice). */
export function bytesToText(b: Uint8Array): string {
  let out = '';
  for (const c of b) out += String.fromCharCode(c);
  return out;
}

export function textToBytes(s: string): Uint8Array {
  return Uint8Array.from(s, (c) => c.charCodeAt(0) & 0xff);
}

class Reader {
  at: number;
  constructor(
    readonly bytes: Uint8Array,
    start: number,
    readonly end: number,
  ) {
    this.at = start;
  }
  byte(): number {
    if (this.at >= this.end) throw new WclapError('bad_module', 'module ends early');
    return this.bytes[this.at++] as number;
  }
  u32(): number {
    let result = 0;
    let shift = 0;
    for (;;) {
      const b = this.byte();
      result += (b & 0x7f) * 2 ** shift;
      if ((b & 0x80) === 0) return result;
      shift += 7;
      if (shift > 35) throw new WclapError('bad_module', 'bad number in module');
    }
  }
  skip(n: number): void {
    if (this.at + n > this.end) throw new WclapError('bad_module', 'module ends early');
    this.at += n;
  }
  name(): string {
    const n = this.u32();
    const start = this.at;
    this.skip(n);
    return bytesToText(this.bytes.subarray(start, start + n));
  }
}

function limits(r: Reader): Omit<MemoryDecl, 'imported'> {
  const flag = r.byte();
  // Bit 2 is the 64-bit memory form, refused (the host only knows 32-bit pointers).
  if (flag > 3) throw new WclapError('unsupported', 'only 32-bit memory is supported');
  const min = r.u32();
  const max = flag & 1 ? r.u32() : null;
  return { minPages: min, maxPages: max, shared: (flag & 2) !== 0 };
}

/** Reads the memory the module imports or declares, straight from the binary (the JS API hides the limits). */
export function readMemoryDecl(bytes: Uint8Array): MemoryDecl | null {
  if (bytes.length < 8 || bytes[0] !== 0 || bytes[1] !== 0x61 || bytes[2] !== 0x73 || bytes[3] !== 0x6d) {
    throw new WclapError('bad_module', 'not a wasm module');
  }
  const r = new Reader(bytes, 8, bytes.length);
  let own: MemoryDecl | null = null;
  while (r.at < bytes.length) {
    const id = r.byte();
    const size = r.u32();
    const end = r.at + size;
    if (end > bytes.length) throw new WclapError('bad_module', 'section runs past the end');
    const s = new Reader(bytes, r.at, end);
    if (id === 2) {
      const count = s.u32();
      for (let i = 0; i < count; i++) {
        s.name();
        s.name();
        const kind = s.byte();
        if (kind === 0) s.u32();
        else if (kind === 1) {
          s.byte();
          const f = s.byte();
          s.u32();
          if (f & 1) s.u32();
        } else if (kind === 2) return { ...limits(s), imported: true };
        else if (kind === 3) s.skip(2);
        else if (kind === 4) {
          s.byte();
          s.u32();
        } else throw new WclapError('bad_module', 'unknown import kind');
      }
    } else if (id === 5 && s.u32() > 0) {
      own = { ...limits(s), imported: false };
    }
    r.at = end;
  }
  return own;
}

export interface Inspection {
  memory: MemoryDecl;
  /** Names of the WASI functions the module imports (all stubbed by the host). */
  wasi: string[];
}

/** Checks the whole file against the plugin contract; throws WclapError with a plain reason. */
export function inspectPlugin(bytes: Uint8Array, module: WebAssembly.Module, lim: WclapLimits): Inspection {
  if (bytes.length > lim.maxFileBytes) {
    throw new WclapError('too_large', `plugin file is ${bytes.length} bytes, the cap is ${lim.maxFileBytes}`);
  }
  const memory = readMemoryDecl(bytes);
  const imports = WebAssembly.Module.imports(module);
  const exports = WebAssembly.Module.exports(module);
  const wasi: string[] = [];
  let importedMemories = 0;
  for (const i of imports) {
    if (i.kind === 'memory') importedMemories++;
    else if (i.kind === 'function' && i.module === WASI_NAMESPACE) wasi.push(i.name);
    else throw new WclapError('imports', `plugin imports ${i.module}.${i.name} (${i.kind}); only memory and WASI functions are allowed`);
  }
  if (importedMemories > 1) throw new WclapError('imports', 'plugin imports more than one memory');
  const ownMemory = exports.some((e) => e.kind === 'memory');
  if (importedMemories === 0 && !ownMemory) throw new WclapError('memory', 'plugin has no memory');
  if (!memory) throw new WclapError('memory', 'plugin has no memory');
  if (memory.imported && !memory.shared) throw new WclapError('memory', 'an imported plugin memory must be shared');
  if (memory.minPages > lim.maxMemoryPages) {
    throw new WclapError('memory', `plugin needs ${memory.minPages} memory pages, the ceiling is ${lim.maxMemoryPages}`);
  }
  const entry = exports.find((e) => e.name === 'clap_entry');
  if (!entry || entry.kind !== 'global') throw new WclapError('exports', 'plugin does not export clap_entry');
  const malloc = exports.find((e) => e.name === 'malloc');
  if (!malloc || malloc.kind !== 'function') throw new WclapError('exports', 'plugin does not export malloc');
  const tables = exports.filter((e) => e.kind === 'table');
  if (tables.length !== 1) throw new WclapError('exports', 'plugin must export exactly one function table');
  return { memory, wasi };
}
