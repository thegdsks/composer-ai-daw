import { describe, expect, it } from 'vitest';
import { acceptDrop, beginDrag, endDrag, getDrag, laneTarget, subscribeDrag, type DragPayload } from '../src/ui/dnd';
import { isMenuKey, menuTarget, placeMenu } from '../src/ui/menuModel';
import { clipMenuItems, soundMenuItems, trackMenuItems } from '../src/ui/menus';
import type { Track } from '../src/model';
import type { SoundEntry } from '../src/backend/types';

const sound: DragPayload = { kind: 'sound', soundId: 'builtin-warm-pad', name: 'Warm Pad' };
const names = (id: string): string | null => ({ a: 'Lead', b: 'Bass' })[id as 'a' | 'b'] ?? null;

describe('drop target resolution', () => {
  it('maps lanes, clips and the strip below the last lane', () => {
    const ids = ['a', 'b'];
    expect(laneTarget(ids, 1, null)).toEqual({ kind: 'track', trackId: 'b' });
    expect(laneTarget(ids, 0, 'r1')).toEqual({ kind: 'clip', trackId: 'a', regionId: 'r1' });
    expect(laneTarget(ids, 2, null)).toEqual({ kind: 'new-track' });
    expect(laneTarget(ids, 0, null)).toEqual({ kind: 'track', trackId: 'a' });
    expect(laneTarget(ids, null, null)).toEqual({ kind: 'none' });
    expect(laneTarget([], 0, null)).toEqual({ kind: 'new-track' });
  });
});

describe('payload accept rules', () => {
  it('lets a sound land on a track, a clip and a new track, and says what happens', () => {
    expect(acceptDrop(sound, { kind: 'track', trackId: 'a' }, names)).toEqual({ ok: true, label: 'Use Warm Pad on Lead' });
    expect(acceptDrop(sound, { kind: 'clip', trackId: 'b', regionId: 'r' }, names)).toEqual({ ok: true, label: 'Use Warm Pad on Bass' });
    expect(acceptDrop(sound, { kind: 'new-track' }, names)).toEqual({ ok: true, label: 'Add a track with Warm Pad' });
  });

  it('refuses a missing track, no target and payloads that have no handler yet', () => {
    expect(acceptDrop(sound, { kind: 'track', trackId: 'gone' }, names).ok).toBe(false);
    expect(acceptDrop(sound, { kind: 'none' }, names).ok).toBe(false);
    expect(acceptDrop({ kind: 'file', fileName: 'x.mid' }, { kind: 'new-track' }, names).ok).toBe(false);
    expect(acceptDrop({ kind: 'clip', regionIds: ['r'] }, { kind: 'track', trackId: 'a' }, names).ok).toBe(false);
  });

  it('tracks the drag in flight and tells subscribers', () => {
    let calls = 0;
    const off = subscribeDrag(() => (calls += 1));
    beginDrag(sound, null);
    expect(getDrag()).toEqual(sound);
    endDrag();
    endDrag();
    expect(getDrag()).toBeNull();
    expect(calls).toBe(2);
    off();
  });
});

describe('menu keyboard navigation', () => {
  const labels = ['Rename track', 'Move up', 'Move down', 'Delete track'];
  it('wraps arrows and jumps with Home and End', () => {
    expect(menuTarget(labels, -1, 'ArrowDown')).toBe(0);
    expect(menuTarget(labels, 3, 'ArrowDown')).toBe(0);
    expect(menuTarget(labels, 0, 'ArrowUp')).toBe(3);
    expect(menuTarget(labels, -1, 'ArrowUp')).toBe(3);
    expect(menuTarget(labels, 2, 'Home')).toBe(0);
    expect(menuTarget(labels, 0, 'End')).toBe(3);
  });
  it('jumps to the next label by first letter and ignores other keys', () => {
    expect(menuTarget(labels, 0, 'm')).toBe(1);
    expect(menuTarget(labels, 1, 'm')).toBe(2);
    expect(menuTarget(labels, 2, 'm')).toBe(1);
    expect(menuTarget(labels, 0, 'z')).toBeNull();
    expect(menuTarget(labels, 0, 'Enter')).toBeNull();
    expect(menuTarget([], 0, 'ArrowDown')).toBeNull();
  });
  it('opens from Shift+F10 and the Menu key only', () => {
    const base = { shiftKey: false, ctrlKey: false, metaKey: false, altKey: false };
    expect(isMenuKey({ ...base, key: 'F10', shiftKey: true })).toBe(true);
    expect(isMenuKey({ ...base, key: 'ContextMenu' })).toBe(true);
    expect(isMenuKey({ ...base, key: 'F10' })).toBe(false);
    expect(isMenuKey({ ...base, key: 'F10', shiftKey: true, ctrlKey: true })).toBe(false);
  });
  it('keeps the menu inside the viewport', () => {
    expect(placeMenu({ x: 990, y: 790 }, { w: 200, h: 150 }, { w: 1000, h: 800 }, 8)).toEqual({ x: 792, y: 642 });
    expect(placeMenu({ x: -5, y: 2 }, { w: 200, h: 150 }, { w: 1000, h: 800 }, 8)).toEqual({ x: 8, y: 8 });
  });
});

describe('menu item lists', () => {
  const track = (id: string, name: string): Track => ({ id, name }) as unknown as Track;
  const noop = (): void => undefined;
  const deps = { rename: noop, move: noop, duplicate: noop, remove: noop };

  it('track menu offers rename, move, duplicate and delete, with ends disabled', () => {
    const first = trackMenuItems(track('a', 'Lead'), 0, 2, deps);
    expect(first.map((i) => i.id)).toEqual(['rename', 'move-up', 'move-down', 'duplicate', 'delete']);
    expect(first.find((i) => i.id === 'move-up')?.disabled).toBe(true);
    expect(first.find((i) => i.id === 'move-down')?.disabled).toBe(false);
    expect(trackMenuItems(track('b', 'Bass'), 1, 2, deps).find((i) => i.id === 'move-down')?.disabled).toBe(true);
  });

  it('clip menu names its shortcuts and counts a multi selection', () => {
    const one = clipMenuItems(1, { open: noop, split: noop, duplicate: noop, remove: noop });
    expect(one.map((i) => i.shortcut)).toEqual(['Enter', 'S', 'Ctrl D', 'Delete']);
    const many = clipMenuItems(3, { open: noop, split: noop, duplicate: noop, remove: noop });
    expect(many.find((i) => i.id === 'open')?.disabled).toBe(true);
    expect(many.find((i) => i.id === 'delete')?.label).toBe('Delete 3 clips');
  });

  it('sound menu lists each track then a new track, each picking its drop target', () => {
    const picks: unknown[] = [];
    const entry = { id: 's', name: 'Warm Pad' } as SoundEntry;
    const items = soundMenuItems(entry, [track('a', 'Lead'), track('b', 'Bass')], (t) => picks.push(t));
    expect(items.map((i) => i.label)).toEqual(['Use on Lead', 'Use on Bass', 'Add a track with Warm Pad']);
    items[1]?.run();
    items[2]?.run();
    expect(picks).toEqual([{ kind: 'track', trackId: 'b' }, { kind: 'new-track' }]);
  });
});
