import { describe, expect, it } from 'vitest';
import { EventQueue } from '../src/audio/wclap/events';
import { forwardingModule } from '../src/audio/wclap/hostFn';
import { inspectPlugin, readMemoryDecl, WclapError } from '../src/audio/wclap/inspect';
import { WclapPlugin, type HostOptions } from '../src/audio/wclap/pluginHost';
import { buildGainPlugin } from './fixtures/gainPlugin.mjs';

const limits = { maxFileBytes: 1 << 20, maxMemoryPages: 64 };
const opts = (over: Partial<HostOptions> = {}): HostOptions => ({
  sampleRate: 48000,
  limits,
  maxEvents: 16,
  budgetShare: 0.8,
  overrunLimit: 4,
  now: () => performance.now(),
  ...over,
});
const codeOf = (fn: () => unknown): string | null => {
  try {
    fn();
    return null;
  } catch (e) {
    return e instanceof WclapError ? e.code : 'other';
  }
};
const block = (n: number, v: number): Float32Array => new Float32Array(n).fill(v);

describe('inspect', () => {
  it('reads the declared memory from the binary', () => {
    expect(readMemoryDecl(buildGainPlugin({ memoryPages: 7 }))).toEqual({ minPages: 7, maxPages: null, shared: false, imported: false });
  });

  it('accepts the fixture', () => {
    const b = buildGainPlugin();
    const r = inspectPlugin(b, new WebAssembly.Module(b as BufferSource), limits);
    expect(r.wasi).toEqual([]);
  });

  it('refuses imports outside memory and WASI, missing exports, big files and big memory', () => {
    const check = (b: Uint8Array, lim = limits): string | null => codeOf(() => inspectPlugin(b, new WebAssembly.Module(b as BufferSource), lim));
    expect(check(buildGainPlugin({ badImport: true }))).toBe('imports');
    expect(check(buildGainPlugin({ omit: 'entry' }))).toBe('exports');
    expect(check(buildGainPlugin({ omit: 'malloc' }))).toBe('exports');
    expect(check(buildGainPlugin(), { ...limits, maxFileBytes: 100 })).toBe('too_large');
    expect(check(buildGainPlugin({ memoryPages: 65 }))).toBe('memory');
  });

  it('refuses bytes that are not wasm', () => {
    expect(codeOf(() => readMemoryDecl(new Uint8Array([1, 2, 3])))).toBe('bad_module');
    expect(codeOf(() => WclapPlugin.load(new Uint8Array(16), opts()))).toBe('bad_module');
  });
});

describe('host function forwarding', () => {
  it('makes a valid module for each signature', () => {
    for (const sig of ['i', 'ii:i', 'i:i', ':', 'ildf:d']) expect(WebAssembly.validate(forwardingModule(sig) as BufferSource)).toBe(true);
  });
});

describe('event queue', () => {
  it('delivers same time events in arrival order and holds later ones for the next block', () => {
    const q = new EventQueue(8);
    q.push(5, 1, 0.1);
    q.push(0, 2, 0.2);
    q.push(5, 3, 0.3);
    q.push(200, 4, 0.4);
    expect(q.take(128).map((e) => e.id)).toEqual([2, 1, 3]);
    expect(q.take(128).map((e) => [e.id, e.time])).toEqual([[4, 72]]);
  });

  it('counts events it refused instead of dropping them silently', () => {
    const q = new EventQueue(1);
    expect(q.push(0, 1, 0)).toBe(true);
    expect(q.push(0, 2, 0)).toBe(false);
    expect(q.dropped).toBe(1);
  });
});

describe('plugin host', () => {
  it('starts the fixture through the CLAP entry, factory and plugin calls', () => {
    const p = WclapPlugin.load(buildGainPlugin(), opts());
    expect(p.id).toBe('wclap.fixture.gain');
    expect(p.name).toBe('Fixture gain');
    expect(p.params).toEqual([{ id: 0, name: 'Gain', min: 0, max: 2, def: 1 }]);
    expect(p.latencyFrames).toBe(0);
    expect(p.getParam(0)).toBe(1);
  });

  it('passes audio at unity gain and applies a parameter change', () => {
    const p = WclapPlugin.load(buildGainPlugin(), opts());
    const l = block(300, 0.5);
    const r = block(300, -0.25);
    p.process(l, r, 0, 300);
    expect(l[0]).toBeCloseTo(0.5, 6);
    expect(r[299]).toBeCloseTo(-0.25, 6);
    p.setParam(0, 0.5);
    const l2 = block(128, 0.5);
    const r2 = block(128, 0.5);
    p.process(l2, r2, 0, 128);
    expect(l2[0]).toBeCloseTo(0.25, 6);
    expect(r2[127]).toBeCloseTo(0.25, 6);
    expect(p.getParam(0)).toBeCloseTo(0.5, 6);
    const s = p.stats();
    expect(s.blocks).toBe(4);
    expect(s.outPeak).toBeGreaterThan(0);
    expect(s.bypassed).toBeNull();
  });

  it('processes in place at an offset inside a larger buffer', () => {
    const p = WclapPlugin.load(buildGainPlugin(), opts());
    p.setParam(0, 2);
    const l = block(10, 1);
    p.process(l, block(10, 1), 4, 3);
    expect(Array.from(l)).toEqual([1, 1, 1, 1, 2, 2, 2, 1, 1, 1]);
  });

  it('bypasses a plugin after it is killed and leaves the audio alone', () => {
    const p = WclapPlugin.load(buildGainPlugin(), opts());
    p.setParam(0, 0);
    p.kill('test');
    const l = block(128, 0.5);
    p.process(l, block(128, 0.5), 0, 128);
    expect(l[0]).toBe(0.5);
    expect(p.bypassed).toBe('test');
  });

  it('bypasses a plugin that keeps going over its time budget', () => {
    let t = 0;
    const p = WclapPlugin.load(buildGainPlugin(), opts({ now: () => (t += 10), overrunLimit: 3 }));
    for (let i = 0; i < 5; i++) p.process(block(128, 0.1), block(128, 0.1), 0, 128);
    expect(p.bypassed).toMatch(/budget/);
    expect(p.stats().overruns).toBe(3);
  });

  it('measures the cost of a block', () => {
    const p = WclapPlugin.load(buildGainPlugin(), opts());
    for (let i = 0; i < 50; i++) p.process(block(128, 0.1), block(128, 0.1), 0, 128);
    const s = p.stats();
    expect(s.avgBlockUs).toBeGreaterThanOrEqual(0);
    expect(s.avgBlockUs).toBeLessThan((128 / 48000) * 1e6);
  });
});
