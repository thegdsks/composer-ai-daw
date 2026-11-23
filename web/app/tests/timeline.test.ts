// Timeline lanes (U39): tap tempo, position readout, marker and lane plans, and the mirror
// following markers and clip loops through the real wasm editor.
import { describe, expect, it } from 'vitest';
import { ProjectMirror } from '../src/backend/mirror';
import type { Project, TempoMap } from '../src/model';
import { formatPosition, formatSeconds } from '../src/state/position';
import { TapTempo } from '../src/state/tapTempo';
import {
  addMarker,
  hitPoint,
  loopChoices,
  loopText,
  markerPoints,
  nextMarkerName,
  removeTempoAt,
  renameMarker,
  setLoop,
  tempoAt,
  tempoPoints,
} from '../src/state/timelinePlans';
import { counterIds, newBackend } from './helpers';

const opts = { resetMs: 2000, window: 4, minTaps: 2 };
const map: TempoMap = { tempos: { '0': 120000 }, meters: { '0': { numerator: 4, denominator: 4 } } };

describe('tap tempo', () => {
  it('reports nothing for one tap and the average for several', () => {
    const t = new TapTempo(opts);
    expect(t.tap(1000)).toBeNull();
    expect(t.tap(1500)).toBeCloseTo(120, 5);
    expect(t.tap(2000)).toBeCloseTo(120, 5);
    // Gaps of 500, 500 and 600 average to 533.33 ms.
    expect(t.tap(2600)).toBeCloseTo(60000 / ((500 + 500 + 600) / 3), 5);
  });

  it('averages only the last taps in the window', () => {
    const t = new TapTempo({ ...opts, window: 3 });
    t.tap(0);
    t.tap(1000);
    t.tap(1500);
    expect(t.tap(2000)).toBeCloseTo(120, 5);
    expect(t.count()).toBe(3);
  });

  it('starts over after a gap longer than the reset', () => {
    const t = new TapTempo(opts);
    t.tap(0);
    t.tap(500);
    expect(t.tap(5000)).toBeNull();
    expect(t.tap(5400)).toBeCloseTo(150, 5);
  });

  it('keeps a gap equal to the reset inside the same count', () => {
    const t = new TapTempo(opts);
    t.tap(0);
    expect(t.tap(2000)).toBeCloseTo(30, 5);
  });

  it('ignores a repeated timestamp instead of dividing by zero', () => {
    const t = new TapTempo(opts);
    t.tap(100);
    t.tap(600);
    expect(t.tap(600)).toBeCloseTo(120, 5);
  });
});

describe('position readout', () => {
  it('formats seconds with hundredths and hours', () => {
    expect(formatSeconds(0)).toBe('0:00.00');
    expect(formatSeconds(8.254)).toBe('0:08.25');
    expect(formatSeconds(75.5)).toBe('1:15.50');
    expect(formatSeconds(3725.01)).toBe('1:02:05.01');
    expect(formatSeconds(-3)).toBe('0:00.00');
  });

  it('counts bars, beats and sixteenths from one', () => {
    expect(formatPosition(map, 0).musical).toBe('1.1.1');
    expect(formatPosition(map, 960).musical).toBe('1.2.1');
    expect(formatPosition(map, 960 + 240 * 2).musical).toBe('1.2.3');
    expect(formatPosition(map, 3840 * 4).musical).toBe('5.1.1');
  });

  it('gives minutes and seconds from the tempo', () => {
    // 120 BPM is half a second per beat, so one bar is two seconds.
    expect(formatPosition(map, 3840).time).toBe('0:02.00');
    expect(formatPosition(map, 3840 * 30).time).toBe('1:00.00');
  });

  it('follows a meter change and a tempo change', () => {
    const m: TempoMap = {
      tempos: { '0': 120000, '3840': 60000 },
      meters: { '0': { numerator: 4, denominator: 4 }, '3840': { numerator: 3, denominator: 4 } },
    };
    expect(formatPosition(m, 3840 + 2880).musical).toBe('3.1.1');
    // One bar at 120 BPM (2 s), then one beat at 60 BPM (1 s).
    expect(formatPosition(m, 3840 + 960).time).toBe('0:03.00');
  });
});

describe('marker plans', () => {
  it('names the next marker by count and skips used names', () => {
    expect(nextMarkerName({})).toBe('Marker 1');
    expect(nextMarkerName({ markers: { '0': 'Marker 2' } })).toBe('Marker 3');
    expect(nextMarkerName({ markers: { '0': 'Marker 1', '960': 'Marker 2' } })).toBe('Marker 3');
  });

  it('adds at a tick, and not where one already is', () => {
    const p = { markers: { '3840': 'Verse' } };
    expect(addMarker(p, 7680)).toEqual({ command: 'set_marker', tick: 7680, name: 'Marker 2' });
    expect(addMarker(p, 7680, '  Chorus ')).toEqual({ command: 'set_marker', tick: 7680, name: 'Chorus' });
    expect(addMarker(p, 3840)).toBeNull();
  });

  it('renames only to a new non empty name', () => {
    const p = { markers: { '3840': 'Verse' } };
    expect(renameMarker(p, 3840, 'Bridge')).toEqual({ command: 'set_marker', tick: 3840, name: 'Bridge' });
    expect(renameMarker(p, 3840, 'Verse')).toBeNull();
    expect(renameMarker(p, 3840, '   ')).toBeNull();
  });

  it('lists markers and tempo points in time order', () => {
    expect(markerPoints({ markers: { '960': 'b', '0': 'a', '3840': 'c' } }).map((m) => m.label)).toEqual(['a', 'b', 'c']);
    expect(tempoPoints({ ...map, tempos: { '3840': 90500, '0': 120000 } })).toEqual([
      { tick: 0, label: '120' },
      { tick: 3840, label: '90.5' },
    ]);
  });

  it('clamps tempo and keeps the first tempo', () => {
    expect(tempoAt(15360, 90)).toEqual({ command: 'set_tempo', tick: 15360, mbpm: 90000 });
    expect(tempoAt(0, 100000)).toMatchObject({ mbpm: expect.any(Number) });
    expect(tempoAt(0, Number.NaN)).toBeNull();
    expect(removeTempoAt(0)).toBeNull();
    expect(removeTempoAt(3840)).toEqual({ command: 'remove_tempo', tick: 3840 });
  });

  it('hits the nearest point within the grab distance', () => {
    const pts = [
      { tick: 0, label: 'a' },
      { tick: 1000, label: 'b' },
    ];
    // 0.1 px per tick: b sits at 100 px.
    expect(hitPoint(pts, 103, 0, 0.1, 8)).toBe(1);
    expect(hitPoint(pts, 50, 0, 0.1, 8)).toBe(-1);
    expect(hitPoint(pts, 4, 0, 0.1, 8)).toBe(0);
    expect(hitPoint([], 4, 0, 0.1, 8)).toBe(-1);
  });
});

describe('clip loop plans', () => {
  it('offers only lengths shorter than the clip', () => {
    const c = loopChoices(960, 3840 * 2, [1, 4, 8, 16]);
    expect(c.map((x) => x.beats)).toEqual([1, 4]);
    expect(setLoop('r', 960.4)).toEqual({ command: 'set_region_loop', region: 'r', loop_length: 960 });
    expect(setLoop('r', -5)).toMatchObject({ loop_length: 0 });
  });

  it('describes a loop in plain words', () => {
    expect(loopText(0, 960, 3840)).toBe('Off');
    expect(loopText(3840, 960, 3840)).toBe('1 bar');
    expect(loopText(7680, 960, 3840)).toBe('2 bars');
    expect(loopText(2880, 960, 3840)).toBe('3 beats');
    expect(loopText(960, 960, 3840)).toBe('1 beat');
  });
});

describe('mirror with markers and loops (real editor)', () => {
  it('follows set_marker, remove_marker, set_region_loop and their undo', async () => {
    const backend = newBackend('timeline');
    const mirror = new ProjectMirror(backend);
    await mirror.open();
    const id = counterIds();
    const [track, region] = [id(), id()];
    await mirror.group('Clip', [
      { command: 'add_track', id: track, name: 'A', kind: 'instrument', index: null },
      { command: 'add_region', id: region, track, start: 0, length: 15360, kind: 'midi' },
    ]);
    expect((await mirror.apply({ command: 'set_marker', tick: 3840, name: 'Verse' })).ok).toBe(true);
    expect((mirror.project as Project).markers).toEqual({ '3840': 'Verse' });
    await mirror.apply({ command: 'set_marker', tick: 3840, name: 'Chorus' });
    expect((mirror.project as Project).markers).toEqual({ '3840': 'Chorus' });
    await mirror.apply({ command: 'set_region_loop', region, loop_length: 3840 });
    expect((mirror.project as Project).tracks[0]?.regions[region]?.loop_length).toBe(3840);
    expect(await mirror.verify()).toBe(true);
    await mirror.undo();
    expect((mirror.project as Project).tracks[0]?.regions[region]?.loop_length).toBeUndefined();
    await mirror.apply({ command: 'remove_marker', tick: 3840 });
    expect((mirror.project as Project).markers).toBeUndefined();
    await mirror.undo();
    expect((mirror.project as Project).markers).toEqual({ '3840': 'Chorus' });
    expect(await mirror.verify()).toBe(true);
    expect(mirror.stats.fallbacks).toBe(0);
    backend.dispose();
  });
});
