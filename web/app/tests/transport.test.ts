import { describe, expect, it } from 'vitest';
import { ClickBank } from '../src/audio/click';
import { buildPlan } from '../src/audio/plan';
import { starterCommands } from '../src/fixtures';
import type { TempoMap } from '../src/model';
import { DEFAULT_TRANSPORT, Store } from '../src/state/store';
import {
  barAt,
  barRangeText,
  beatClicks,
  braceFromDrag,
  countInClicks,
  loopActive,
  lowerBound,
  rulerTick,
  segmentFrames,
  wrapTick,
} from '../src/time';
import { BLOCK_FRAMES, WasmEngine } from '../src/wasm/engine';
import { counterIds, newBackend, wasmModule } from './helpers';

const map: TempoMap = { tempos: { '0': 120000 }, meters: { '0': { numerator: 4, denominator: 4 } } };
const SR = 48000;
const BAR = 3840;

describe('loop and seek math', () => {
  it('folds a tick past the loop end back into the loop', () => {
    const loop = { on: true, start: BAR, end: 3 * BAR };
    expect(wrapTick(BAR - 1, loop)).toBe(BAR - 1);
    expect(wrapTick(3 * BAR - 1, loop)).toBe(3 * BAR - 1);
    expect(wrapTick(3 * BAR, loop)).toBe(BAR);
    expect(wrapTick(3 * BAR + 100, loop)).toBe(BAR + 100);
    expect(wrapTick(8 * BAR, loop)).toBe(2 * BAR);
  });

  it('leaves ticks alone when the loop is off or empty', () => {
    expect(wrapTick(99999, { on: false, start: 0, end: BAR })).toBe(99999);
    expect(wrapTick(99999, { on: true, start: BAR, end: BAR })).toBe(99999);
    expect(loopActive({ on: true, start: 5, end: 5 })).toBe(false);
  });

  it('ends an audio segment on the exact loop end frame', () => {
    expect(segmentFrames(1000, 128, 1050)).toBe(50);
    expect(segmentFrames(1000, 128, 1128)).toBe(128);
    expect(segmentFrames(1000, 128, 2000)).toBe(128);
    expect(segmentFrames(1000, 128, null)).toBe(128);
    expect(segmentFrames(2000, 128, 1500)).toBe(128);
  });

  it('turns a ruler point into a snapped tick, never before the start', () => {
    const pxPerTick = 0.1;
    expect(rulerTick(2 * BAR * pxPerTick, 0, pxPerTick, BAR)).toBe(2 * BAR);
    expect(rulerTick(2 * BAR * pxPerTick + 40, 0, pxPerTick, BAR)).toBe(2 * BAR);
    expect(rulerTick(-50, 0, pxPerTick, BAR)).toBe(0);
    expect(rulerTick(0, BAR, pxPerTick, BAR)).toBe(BAR);
  });

  it('orders and snaps a brace drag and keeps at least one step', () => {
    expect(braceFromDrag(2 * BAR, 0, BAR)).toEqual({ start: 0, end: 2 * BAR });
    expect(braceFromDrag(BAR + 10, BAR + 20, BAR)).toEqual({ start: BAR, end: 2 * BAR });
    expect(braceFromDrag(10, 3 * BAR - 5, BAR)).toEqual({ start: 0, end: 3 * BAR });
  });

  it('finds the first beat at or after a frame', () => {
    const beats = [0, 24000, 48000, 72000];
    expect(lowerBound(beats, 0)).toBe(0);
    expect(lowerBound(beats, 24000)).toBe(1);
    expect(lowerBound(beats, 24001)).toBe(2);
    expect(lowerBound(beats, 99999)).toBe(4);
  });

  it('names bars for announcements', () => {
    expect(barAt(map, 0)).toBe(0);
    expect(barAt(map, 2 * BAR + 1)).toBe(2);
    expect(barRangeText(map, 0, 2 * BAR)).toBe('bars 1 to 2');
    expect(barRangeText(map, 2 * BAR, 3 * BAR)).toBe('bar 3');
  });
});

describe('click schedules', () => {
  it('puts a click on every beat with the accent on the bar line', () => {
    const c = beatClicks(map, 0, 2 * BAR, SR);
    expect(c).toHaveLength(8);
    expect(c.map((x) => x.frame)).toEqual([0, 24000, 48000, 72000, 96000, 120000, 144000, 168000]);
    expect(c.map((x) => x.accent)).toEqual([true, false, false, false, true, false, false, false]);
  });

  it('follows a tempo change', () => {
    const fast: TempoMap = { tempos: { '0': 120000, [String(BAR)]: 240000 }, meters: map.meters };
    const c = beatClicks(fast, 0, 2 * BAR, SR);
    expect(c[4]?.frame).toBe(96000);
    expect(c[5]?.frame).toBe(96000 + 12000);
  });

  it('builds a count in of whole bars', () => {
    const one = countInClicks(map, 0, 1, SR);
    expect(one.clicks.map((x) => x.frame)).toEqual([0, 24000, 48000, 72000]);
    expect(one.totalFrames).toBe(96000);
    expect(countInClicks(map, 0, 2, SR).totalFrames).toBe(192000);
    expect(countInClicks(map, 0, 0, SR).clicks).toHaveLength(0);
  });

  it('builds the worklet plan only while the metronome is on', () => {
    const prefs = { ...DEFAULT_TRANSPORT, loop: { on: true, start: BAR, end: 2 * BAR } };
    const off = buildPlan({ map, prefs, endTick: 2 * BAR }, SR);
    expect(off.beats).toHaveLength(0);
    expect(off.loop).toEqual({ on: true, startFrame: 96000, endFrame: 192000 });
    const on = buildPlan({ map, prefs: { ...prefs, metronome: { on: true } }, endTick: 2 * BAR }, SR);
    expect(on.metronome).toBe(true);
    expect(on.beats.length).toBeGreaterThanOrEqual(8);
    expect(on.accents[0]).toBe(1);
    expect(on.accents[1]).toBe(0);
  });
});

describe('click voice', () => {
  const opts = { sampleRate: SR, ms: 28, gain: 0.3, hz: 1000, accentHz: 1600 };

  it('is silent until it starts, sounds, and decays to nothing', () => {
    const bank = new ClickBank(opts);
    const l = new Float32Array(2048);
    const r = new Float32Array(2048);
    bank.render(l, r, 0, 2048);
    expect(Math.max(...l)).toBe(0);
    bank.start(100, true);
    bank.render(l, r, 0, 2048);
    expect(l.slice(0, 100).every((v) => v === 0)).toBe(true);
    expect(Math.max(...l.map(Math.abs))).toBeGreaterThan(0.05);
    expect(Math.abs(l[2047] ?? 1)).toBe(0);
    expect(Array.from(l)).toEqual(Array.from(r));
  });

  it('gives the same samples for the same schedule however the quanta fall', () => {
    const whole = new Float32Array(1024);
    const a = new ClickBank(opts);
    a.start(300, false);
    a.render(whole, new Float32Array(1024), 0, 1024);
    const parts = new Float32Array(1024);
    const b = new ClickBank(opts);
    b.start(300, false);
    b.render(parts, new Float32Array(1024), 0, 512);
    const tail = new Float32Array(512);
    b.render(tail, new Float32Array(512), 0, 512);
    parts.set(tail, 512);
    expect(Array.from(parts)).toEqual(Array.from(whole));
  });
});

describe('engine segments and spare catch up', () => {
  async function blob(): Promise<Uint8Array> {
    const store = new Store(newBackend());
    await store.refresh();
    await store.group('starter', starterCommands(counterIds()));
    const r = await store.backend.engineBlob(SR);
    if (!r.ok) throw new Error(r.error.message);
    return r.value;
  }

  function render(e: WasmEngine, sizes: number[]): Float32Array {
    const out: number[] = [];
    for (const n of sizes) {
      const [l] = e.process(n);
      out.push(...l);
    }
    return Float32Array.from(out);
  }

  it('renders the same samples whether blocks are split at odd sizes or not', async () => {
    const b = await blob();
    const a = WasmEngine.fromModule(wasmModule);
    a.load(b);
    const whole = render(a, Array(16).fill(BLOCK_FRAMES));
    const sizes = [50, 78, 1, 127, 100, 28, 77, 51, 128, 128, 128, 128, 128, 128, 128, 128];
    const d = WasmEngine.fromModule(wasmModule);
    d.load(b);
    const split = render(d, sizes);
    const total = sizes.reduce((x, y) => x + y, 0);
    expect(split.length).toBe(total);
    expect(Array.from(split)).toEqual(Array.from(whole.slice(0, total)));
  });

  it('a spare caught up by skipping lands on the same samples as the engine that played there', async () => {
    const b = await blob();
    const played = WasmEngine.fromModule(wasmModule);
    played.load(b);
    const frames = 40 * BLOCK_FRAMES;
    const all = render(played, Array(40 + 8).fill(BLOCK_FRAMES));
    const spare = WasmEngine.fromModule(wasmModule);
    spare.load(b);
    spare.skip(frames);
    expect(spare.position).toBe(frames);
    const tail = render(spare, Array(8).fill(BLOCK_FRAMES));
    expect(Array.from(tail)).toEqual(Array.from(all.slice(frames)));
  });
});

describe('store transport prefs', () => {
  it('holds the backend answer for loop, metronome, count in and follow', async () => {
    const store = new Store(newBackend());
    await store.refresh();
    expect(store.getSnapshot().transport).toEqual(DEFAULT_TRANSPORT);
    expect(await store.setTransport({ loop: { on: true, start: 0, end: BAR }, metronome: { on: true }, countInBars: 2, follow: false })).toBe(true);
    const t = store.getSnapshot().transport;
    expect(t.loop).toEqual({ on: true, start: 0, end: BAR });
    expect(t.metronome.on).toBe(true);
    expect(t.countInBars).toBe(2);
    expect(t.follow).toBe(false);
  });

  it('refuses a loop that ends before it starts and says why', async () => {
    const store = new Store(newBackend());
    await store.refresh();
    expect(await store.setTransport({ loop: { on: true, start: BAR, end: 10 } })).toBe(false);
    expect(store.getSnapshot().error).not.toBeNull();
    expect(store.getSnapshot().transport.loop.on).toBe(false);
  });
});
