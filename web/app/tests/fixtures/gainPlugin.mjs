// Builds a tiny web plugin (a stereo gain, one parameter) as wasm bytes, so tests need no
// downloaded plugin and no committed binary. About 1 KB. Layout follows the 32-bit CLAP structs.

const uleb = (n) => {
  const out = [];
  do {
    let b = n & 0x7f;
    n >>>= 7;
    if (n) b |= 0x80;
    out.push(b);
  } while (n);
  return out;
};
const sleb = (n) => {
  const out = [];
  for (;;) {
    const b = n & 0x7f;
    n >>= 7;
    if ((n === 0 && !(b & 0x40)) || (n === -1 && b & 0x40)) {
      out.push(b);
      return out;
    }
    out.push(b | 0x80);
  }
};
const vec = (items) => [...uleb(items.length), ...items.flat()];
const section = (id, body) => [id, ...uleb(body.length), ...body];
const name = (s) => [...uleb(s.length), ...Array.from(s, (c) => c.charCodeAt(0))];

// value types and opcodes used below
const I32 = 0x7f;
const F32 = 0x7d;
const F64 = 0x7c;
const op = {
  block: 0x02, loop: 0x03, if: 0x04, end: 0x0b, br: 0x0c, brIf: 0x0d, call: 0x10, callIndirect: 0x11,
  get: 0x20, set: 0x21, gget: 0x23, gset: 0x24,
  i32load: 0x28, f32load: 0x2a, f64load: 0x2b, load8u: 0x2d, load16u: 0x2f, i32store: 0x36, f32store: 0x38, f64store: 0x39,
  i32const: 0x41, f64const: 0x44, eqz: 0x45, eq: 0x46, ne: 0x47, geU: 0x4f, add: 0x6a, and: 0x71, shl: 0x74, f32mul: 0x94, demote: 0xb6, promote: 0xbb,
};
const i32c = (n) => [op.i32const, ...sleb(n)];
const f64c = (x) => [op.f64const, ...new Uint8Array(new Float64Array([x]).buffer)];
const get = (i) => [op.get, i];
const set = (i) => [op.set, i];
const ld = (o, a = 2) => [op.i32load, a, ...uleb(o)];
const st = (o, a = 2) => [op.i32store, a, ...uleb(o)];

// function types
const T = {
  p: { params: [I32], results: [I32] },
  v: { params: [I32], results: [] },
  n: { params: [], results: [] },
  pp: { params: [I32, I32], results: [I32] },
  ppp: { params: [I32, I32, I32], results: [I32] },
  act: { params: [I32, F64, I32, I32], results: [I32] },
  flush: { params: [I32, I32, I32], results: [] },
};
const typeNames = Object.keys(T);
const typeIndex = (k) => typeNames.indexOf(k);

// memory map
const M = { clapId: 1024, plugId: 1056, plugName: 1088, params: 1120, entry: 2048, factory: 2112, desc: 2176, plugin: 2304, ext: 2400 };

export function buildGainPlugin(opts = {}) {
  const imp = opts.badImport ? 1 : 0;
  const applyIdx = imp + 20;
  const mallocIdx = imp + 21;
  const ret = (n) => [...i32c(n), op.end];
  const nop = [op.end];

  // events: apply every param value event for id 0 to the gain global
  const applyEvents = {
    type: 'v',
    locals: [[3, I32]],
    body: [
      get(0), get(0), ...ld(4), op.callIndirect, typeIndex('p'), 0, ...set(1),
      ...i32c(0), ...set(2),
      op.block, 0x40, op.loop, 0x40,
      ...get(2), ...get(1), op.geU, op.brIf, 1,
      ...get(0), ...get(2), ...get(0), ...ld(8), op.callIndirect, typeIndex('pp'), 0, ...set(3),
      ...get(3), op.load16u, 1, 10, ...i32c(5), op.eq,
      ...get(3), ...ld(16), op.eqz, op.and,
      op.if, 0x40, ...get(3), op.f64load, 3, 40, op.demote, op.gset, 0, op.end,
      ...get(2), ...i32c(1), op.add, ...set(2),
      op.br, 0, op.end, op.end, op.end,
    ].flat(),
  };

  const chan = (buf, local) => [
    ...get(buf), ...get(6), ...i32c(2), op.shl, op.add, ...ld(0), ...set(local),
  ];
  const processBody = {
    type: 'pp',
    locals: [[7, I32]],
    body: [
      ...get(1), ...ld(32), op.call, applyIdx,
      ...get(1), ...ld(8), ...set(2),
      ...get(1), ...ld(16), ...ld(0), ...set(3),
      ...get(1), ...ld(20), ...ld(0), ...set(4),
      ...i32c(0), ...set(6),
      op.block, 0x40, op.loop, 0x40,
      ...get(6), ...i32c(2), op.geU, op.brIf, 1,
      ...chan(3, 7), ...chan(4, 8),
      ...i32c(0), ...set(5),
      op.block, 0x40, op.loop, 0x40,
      ...get(5), ...get(2), op.geU, op.brIf, 1,
      ...get(8), ...get(5), ...i32c(2), op.shl, op.add,
      ...get(7), ...get(5), ...i32c(2), op.shl, op.add, op.f32load, 2, 0,
      op.gget, 0, op.f32mul, op.f32store, 2, 0,
      ...get(5), ...i32c(1), op.add, ...set(5),
      op.br, 0, op.end, op.end,
      ...get(6), ...i32c(1), op.add, ...set(6),
      op.br, 0, op.end, op.end,
      ...i32c(1), op.end,
    ].flat(),
  };

  const funcs = [
    { type: 'p', body: ret(1) }, // 1 entry init
    { type: 'n', body: nop }, // 2 entry deinit
    { type: 'p', body: ret(M.factory) }, // 3 get_factory
    { type: 'p', body: ret(1) }, // 4 plugin count
    { type: 'pp', body: ret(M.desc) }, // 5 descriptor
    { type: 'ppp', body: ret(M.plugin) }, // 6 create
    { type: 'p', body: ret(1) }, // 7 init
    { type: 'v', body: nop }, // 8 destroy
    { type: 'act', body: ret(1) }, // 9 activate
    { type: 'v', body: nop }, // 10 deactivate
    { type: 'p', body: ret(1) }, // 11 start
    { type: 'v', body: nop }, // 12 stop
    { type: 'v', body: nop }, // 13 reset
    processBody, // 14 process
    // 15 get_extension: the params table when the id has a 'p' at index 5 (clap.params)
    { type: 'pp', body: [...get(1), op.load8u, 0, 5, ...i32c(0x70), op.eq, op.if, I32, ...i32c(M.ext), 0x05, ...i32c(0), op.end, op.end].flat() },
    { type: 'v', body: nop }, // 16 on_main_thread
    { type: 'p', body: ret(1) }, // 17 params count
    // 18 params get_info (idx 0 only): id 0, flags 1, name "Gain", min 0, max 2, default 1
    {
      type: 'ppp',
      body: [
        ...get(1), op.eqz, op.if, I32,
        ...get(2), ...i32c(0), ...st(0),
        ...get(2), ...i32c(1), ...st(4),
        ...get(2), ...i32c(0x6e696147), ...st(12),
        ...get(2), ...f64c(0), op.f64store, 3, ...uleb(1296),
        ...get(2), ...f64c(2), op.f64store, 3, ...uleb(1304),
        ...get(2), ...f64c(1), op.f64store, 3, ...uleb(1312),
        ...i32c(1), 0x05, ...i32c(0), op.end, op.end,
      ].flat(),
    },
    // 19 params get_value
    { type: 'ppp', body: [...get(2), op.gget, 0, op.promote, op.f64store, 3, 0, ...i32c(1), op.end].flat() },
    // 20 params flush
    { type: 'flush', body: [...get(1), op.call, applyIdx, op.end].flat() },
  ];
  const all = [...funcs, applyEvents, {
    type: 'p',
    locals: [[1, I32]],
    body: [
      op.gget, 1, ...set(1), op.gget, 1, ...get(0), op.add, ...i32c(15), op.add, ...i32c(-16), op.and, op.gset, 1, ...get(1), op.end,
    ].flat(),
  }];

  const types = vec(typeNames.map((k) => [0x60, ...vec(T[k].params.map((t) => [t])), ...vec(T[k].results.map((t) => [t]))]));
  const importSec = imp ? section(2, vec([[...name('env'), ...name('escape'), 0x00, ...uleb(typeIndex('n'))]])) : [];
  const funcSec = section(3, vec(all.map((f) => uleb(typeIndex(f.type)))));
  const tableSize = funcs.length + 1;
  const tableSec = section(4, vec([[0x70, 0x00, ...uleb(tableSize)]]));
  const pages = opts.memoryPages ?? 4;
  const memSec = section(5, vec([[0x00, ...uleb(pages)]]));
  const globalSec = section(6, vec([
    [F32, 1, 0x43, 0, 0, 0x80, 0x3f, op.end],
    [I32, 1, ...i32c(8192), op.end],
    [I32, 0, ...i32c(M.entry), op.end],
  ]));
  const exp = [
    [...name('memory'), 0x02, 0],
    [...name('__indirect_function_table'), 0x01, 0],
    ...(opts.omit === 'entry' ? [] : [[...name('clap_entry'), 0x03, 2]]),
    ...(opts.omit === 'malloc' ? [] : [[...name('malloc'), 0x00, ...uleb(mallocIdx)]]),
  ];
  const exportSec = section(7, vec(exp));
  const elemSec = section(9, vec([[0x00, ...i32c(1), op.end, ...vec(funcs.map((_, i) => uleb(imp + i)))]]));
  const code = vec(all.map((f) => {
    const body = [...vec((f.locals ?? []).map(([n, t]) => [...uleb(n), t])), ...f.body];
    return [...uleb(body.length), ...body];
  }));
  const codeSec = section(10, code);

  const bytes = new Uint8Array(4096);
  const dv = new DataView(bytes.buffer);
  const put = (at, s) => Array.from(s, (c, i) => (bytes[at + i] = c.charCodeAt(0)));
  put(M.clapId, 'clap.plugin-factory');
  put(M.plugId, 'wclap.fixture.gain');
  put(M.plugName, 'Fixture gain');
  put(M.params, 'clap.params');
  const w = (at, ...words) => words.forEach((v, i) => dv.setUint32(at + i * 4, v, true));
  w(M.entry, 1, 2, 10, 1, 2, 3);
  w(M.factory, 4, 5, 6);
  w(M.desc, 1, 2, 10, M.plugId, M.plugName);
  w(M.plugin, M.desc, 0, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);
  w(M.ext, 17, 18, 19, 0, 0, 20);
  // one data segment covering 1024..4096 keeps the module small enough to read in a test
  const dataSec = section(11, vec([[0x00, ...i32c(1024), op.end, ...vec(Array.from(bytes.subarray(1024, 2560)).map((b) => [b]))]]));

  return Uint8Array.from([
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    ...section(1, types), ...importSec, ...funcSec, ...tableSec, ...memSec, ...globalSec, ...exportSec, ...elemSec, ...codeSec, ...dataSec,
  ]);
}
