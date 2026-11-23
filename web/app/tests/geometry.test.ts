import { describe, expect, it } from 'vitest';
import type { Note } from '../src/model';
import { snapStep } from '../src/roll/regionDrag';
import { snapRound } from '../src/time';
import {
  barsText,
  clipBars,
  clipNearTick,
  dropLane,
  fitView,
  hitClip,
  hitNote,
  hitVelocityBar,
  laneAt,
  neighbourClip,
  overlapsOther,
  scrollFromThumb,
  scrollThumb,
  type LaneView,
  pitchToY,
  tickToX,
  velocityFromY,
  visibleRange,
  xToTick,
  yToPitch,
  zoomAround,
  type Viewport,
} from '../src/roll/geometry';

const vp: Viewport = { scrollTick: 0, scrollY: 0, pxPerTick: 0.1, rowHeight: 10, width: 800, height: 400 };
const note = (id: string, pitch: number, start: number, duration: number): Note => ({
  id,
  pitch,
  start,
  duration,
  velocity: 100,
});

describe('coordinates', () => {
  it('maps ticks and pitches both ways', () => {
    expect(tickToX(vp, 960)).toBeCloseTo(96);
    expect(xToTick(vp, 96)).toBeCloseTo(960);
    expect(pitchToY(vp, 127)).toBe(0);
    expect(pitchToY(vp, 126)).toBe(10);
    expect(yToPitch(vp, 0)).toBe(127);
    expect(yToPitch(vp, 15)).toBe(126);
    const scrolled = { ...vp, scrollY: 100 };
    expect(yToPitch(scrolled, 0)).toBe(117);
    expect(pitchToY(scrolled, 117)).toBe(0);
  });

  it('clamps pitch to the MIDI range', () => {
    expect(yToPitch(vp, -50)).toBe(127);
    expect(yToPitch(vp, 99999)).toBe(0);
  });

  it('keeps the anchor tick still while zooming', () => {
    const base = { ...vp, scrollTick: 500 };
    const z = zoomAround(base, 200, 0.4);
    expect(xToTick(z, 200)).toBeCloseTo(xToTick(base, 200));
    expect(z.pxPerTick).toBe(0.4);
  });
});

describe('hit testing', () => {
  const notes = [note('a', 60, 0, 960), note('b', 60, 480, 960), note('c', 62, 0, 100)];
  const y60 = pitchToY(vp, 60) + 5;

  it('finds a note body and its right edge', () => {
    expect(hitNote(notes, 0, vp, 20, y60, 6)?.zone).toBe('body');
    const edge = hitNote(notes.slice(0, 1), 0, vp, 95, y60, 6);
    expect(edge?.note.id).toBe('a');
    expect(edge?.zone).toBe('edge');
  });

  it('prefers the note drawn last where notes overlap', () => {
    expect(hitNote(notes, 0, vp, 60, y60, 6)?.note.id).toBe('b');
  });

  it('misses empty space and other rows', () => {
    expect(hitNote(notes, 0, vp, 20, pitchToY(vp, 70) + 5, 6)).toBeNull();
    expect(hitNote(notes, 0, vp, 300, y60, 6)).toBeNull();
  });

  it('offsets by the region start', () => {
    expect(hitNote([note('r', 60, 0, 960)], 1000, vp, 110, y60, 6)?.note.id).toBe('r');
    expect(hitNote([note('r', 60, 0, 960)], 1000, vp, 20, y60, 6)).toBeNull();
  });

  it('keeps a tiny note grabbable by its body', () => {
    expect(hitNote([note('t', 60, 0, 40)], 0, vp, 1, y60, 6)?.zone).toBe('body');
  });

  it('finds a velocity bar near its x', () => {
    expect(hitVelocityBar(notes, 0, vp, 49, 4)?.id).toBe('b');
    expect(hitVelocityBar(notes, 0, vp, 400, 4)).toBeNull();
  });

  it('maps lane height to velocity', () => {
    expect(velocityFromY(0, 60)).toBe(127);
    expect(velocityFromY(60, 60)).toBe(1);
    expect(velocityFromY(30, 60)).toBe(64);
  });
});

describe('visibleRange', () => {
  const sorted = Array.from({ length: 1000 }, (_, i) => note(String(i), 60, i * 100, 250));
  it('returns every note that can touch the span and few others', () => {
    const [lo, hi] = visibleRange(sorted, 250, 50000, 50500);
    const ids = sorted.slice(lo, hi).map((n) => Number(n.id));
    for (const n of sorted) {
      if (n.start + n.duration > 50000 && n.start < 50500) expect(ids).toContain(Number(n.id));
    }
    expect(ids.length).toBeLessThan(12);
  });
});

describe('arrangement hit tests', () => {
  const lanes: LaneView = { top: 28, rowHeight: 64, scrollY: 0, laneCount: 3 };

  it('finds the lane under a point, the new track strip, and nothing over the ruler', () => {
    expect(laneAt(lanes, 10)).toBeNull();
    expect(laneAt(lanes, 28)).toEqual({ lane: 0, below: false });
    expect(laneAt(lanes, 28 + 64 * 2 + 63)).toEqual({ lane: 2, below: false });
    expect(laneAt(lanes, 28 + 64 * 3)).toEqual({ lane: 3, below: true });
    // Scrolling the track list shifts the lanes under the same canvas y.
    expect(laneAt({ ...lanes, scrollY: 64 }, 28)).toEqual({ lane: 1, below: false });
    expect(dropLane(lanes, 4)).toBe(0);
  });

  const clips = [
    { id: 'a', start: 0, length: 1000 },
    { id: 'b', start: 500, length: 1000 },
  ];

  it('hits the clip body, its right edge, and the topmost of two overlapping clips', () => {
    // 0.1 px per tick: clip a spans px 0..100, clip b spans px 50..150.
    expect(hitClip(clips, 0, 0.1, 20, 8)).toEqual({ clip: clips[0], zone: 'body' });
    expect(hitClip(clips, 0, 0.1, 148, 8)).toEqual({ clip: clips[1], zone: 'edge' });
    expect(hitClip(clips, 0, 0.1, 70, 8)?.clip.id).toBe('b');
    expect(hitClip(clips, 0, 0.1, 160, 8)).toBeNull();
    // A scrolled view moves the clips left.
    expect(hitClip(clips, 500, 0.1, 10, 8)?.clip.id).toBe('b');
    // A narrow clip keeps half its width as body.
    expect(hitClip([{ id: 'n', start: 0, length: 100 }], 0, 0.1, 2, 8)?.zone).toBe('body');
  });

  it('snaps to bars, beats or nothing at three zoom levels', () => {
    const sig = { numerator: 4, denominator: 4 };
    const bar = 3840;
    // 0.05 px per tick: a bar is 192 px. 0.0125: 48 px, still a bar. 0.005: 19 px, so a beat.
    expect(snapStep(sig, 0.05, false, 48)).toBe(bar);
    expect(snapStep(sig, 0.0125, false, 48)).toBe(bar);
    expect(snapStep(sig, 0.005, false, 48)).toBe(960);
    expect(snapStep(sig, 0.005, true, 48)).toBe(1);
    expect(snapRound(bar * 1.4, snapStep(sig, 0.05, false, 48))).toBe(bar);
    expect(snapRound(bar * 1.4, snapStep(sig, 0.005, false, 48))).toBe(bar + 1920);
  });
});

describe('arrangement view math', () => {
  it('fits a span into the window with padding and respects the zoom limits', () => {
    const v = fitView(3840, 3840 * 5, 1000, 20, 0.001, 1);
    expect(v.pxPerTick * 3840 * 4).toBeCloseTo(960, 5);
    expect(v.scrollTick).toBeCloseTo(3840 - 20 / v.pxPerTick, 5);
    expect(fitView(0, 10, 1000, 20, 0.001, 1).pxPerTick).toBe(1);
    expect(fitView(0, 1e9, 1000, 20, 0.001, 1).pxPerTick).toBe(0.001);
    expect(fitView(0, 3840, 1000, 20, 0.001, 1).scrollTick).toBe(0);
  });

  it('sizes the scrollbar thumb and maps it back to a scroll position', () => {
    expect(scrollThumb(0, 1000, 800, 500, 24)).toBeNull();
    expect(scrollThumb(0, 1000, 4000, 400, 24)).toEqual({ x: 0, width: 100 });
    expect(scrollThumb(3000, 1000, 4000, 400, 24)?.x).toBe(300);
    expect(scrollFromThumb(150, 100, 1000, 4000, 400)).toBe(1500);
    expect(scrollFromThumb(-50, 100, 1000, 4000, 400)).toBe(0);
    expect(scrollThumb(0, 1, 1e6, 400, 24)?.width).toBe(24);
  });

  it('names the bars of a clip', () => {
    expect(clipBars(3840 * 4, 3840 * 4, 3840)).toEqual({ first: 5, last: 8 });
    expect(barsText(5, 8)).toBe('bars 5 to 8');
    expect(barsText(3, 3)).toBe('bar 3');
  });

  it('walks to neighbours and nearest clips, and spots overlaps', () => {
    const cs = [
      { id: 'b', start: 3840, length: 3840 },
      { id: 'a', start: 0, length: 3840 },
      { id: 'c', start: 7680, length: 3840 },
    ];
    expect(neighbourClip(cs, 'a', 1)?.id).toBe('b');
    expect(neighbourClip(cs, 'a', -1)).toBeNull();
    expect(neighbourClip(cs, 'c', -1)?.id).toBe('b');
    expect(clipNearTick(cs, 4000)?.id).toBe('b');
    expect(clipNearTick(cs, 20000)?.id).toBe('c');
    expect(overlapsOther([{ id: 'x', start: 0, length: 100 }, { id: 'y', start: 99, length: 10 }], 'x')).toBe(true);
    expect(overlapsOther([{ id: 'x', start: 0, length: 100 }, { id: 'y', start: 100, length: 10 }], 'x')).toBe(false);
  });
});
