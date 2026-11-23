import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { findRegion, Store } from '../src/state/store';
import type { Actions } from '../src/state/actions';
import { minRegionLength, movedStart, regionZone, resizedLength, snapStep } from '../src/roll/regionDrag';
import { plainError } from '../src/state/errors';
import { projectFileName } from '../src/persistence';
import { counterIds, newActions, newBackend } from './helpers';

const sig = { numerator: 4, denominator: 4 };
const BAR = 3840;

describe('region drag math', () => {
  it('picks resize on the right edge and move elsewhere', () => {
    expect(regionZone(195, 100, 100, 8)).toBe('resize');
    expect(regionZone(150, 100, 100, 8)).toBe('move');
    // A very narrow region keeps half of its width for moving.
    expect(regionZone(101, 100, 6, 8)).toBe('move');
  });

  it('snaps to a bar, a beat when zoomed out, and not at all when free', () => {
    expect(snapStep(sig, 0.05, false, 48)).toBe(BAR);
    expect(snapStep(sig, 0.001, false, 48)).toBe(960);
    expect(snapStep(sig, 0.05, true, 48)).toBe(1);
    expect(snapStep({ numerator: 3, denominator: 4 }, 0.05, false, 48)).toBe(2880);
  });

  it('moves with snapping and never before zero', async () => {
    const r = { id: 'a', start: BAR, length: BAR, content: { midi: { notes: {} } } };
    expect(movedStart(r, BAR * 0.6, BAR)).toBe(BAR * 2);
    expect(movedStart(r, BAR * 0.4, BAR)).toBe(BAR);
    expect(movedStart(r, -BAR * 5, BAR)).toBe(0);
  });

  it('resizes with snapping but not below the last note', () => {
    const note = { id: 'n', pitch: 60, velocity: 90, start: 3000, duration: 500 };
    const r = { id: 'a', start: 0, length: BAR * 2, content: { midi: { notes: { n: note } } } };
    expect(minRegionLength(r)).toBe(3500);
    expect(resizedLength(r, BAR, BAR)).toBe(BAR * 3);
    expect(resizedLength(r, -BAR * 2, BAR)).toBe(3500);
  });
});

describe('region move and resize commands', () => {
  async function setup(): Promise<{ store: Store; actions: Actions; id: string }> {
    const store = new Store(newBackend());
    await store.refresh();
    await store.group('starter', starterCommands(counterIds()));
    const actions = newActions(store);
    const snap = store.getSnapshot();
    return { store, actions, id: snap.selectedRegion ?? '' };
  }

  it('moves a region and undoes it', async () => {
    const { store, actions, id } = await setup();
    const region = (): number => findRegion(store.getSnapshot().project, id)!.region.start;
    expect(region()).toBe(0);
    await actions.moveRegion(id, BAR * 2);
    expect(region()).toBe(BAR * 2);
    await store.undo();
    expect(region()).toBe(0);
  });

  it('resizes a region and undoes it', async () => {
    const { store, actions, id } = await setup();
    const length = (): number => findRegion(store.getSnapshot().project, id)!.region.length;
    const before = length();
    await actions.resizeRegion(id, before + BAR);
    expect(length()).toBe(before + BAR);
    await store.undo();
    expect(length()).toBe(before);
  });

  it('reports a refused resize in the error line and leaves the region alone', async () => {
    const { store, actions, id } = await setup();
    const before = findRegion(store.getSnapshot().project, id)!.region.length;
    await actions.resizeRegion(id, 10);
    expect(store.getSnapshot().error).toBeTruthy();
    expect(findRegion(store.getSnapshot().project, id)!.region.length).toBe(before);
  });
});

describe('plain language errors and file names', () => {
  it('explains daemon file codes and keeps unknown ones visible', () => {
    expect(plainError({ code: 'file_not_found', message: 'x' })).toMatch(/no file with that name/i);
    expect(plainError({ code: 'file_exists', message: 'x' })).toMatch(/Replace existing file/);
    expect(plainError({ code: 'bad_magic', message: 'x' })).toMatch(/not a Composer AI DAW project/);
    expect(plainError({ code: 'weird', message: 'odd' })).toBe('weird: odd');
  });

  it('adds the project extension once', () => {
    expect(projectFileName('song')).toBe('song.cmpf');
    expect(projectFileName(' song.CMPF ')).toBe('song.CMPF');
  });
});
