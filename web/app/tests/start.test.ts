import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { nextAction } from '../src/nextAction';
import { parseRoute } from '../src/route';
import { Store } from '../src/state/store';
import { counterIds, newActions, newBackend } from './helpers';

async function fresh(name: string): Promise<Store> {
  const store = new Store(newBackend(name));
  await store.refresh();
  return store;
}

describe('routes', () => {
  it('reads the start and project hashes', () => {
    expect(parseRoute('#/start')).toEqual({ kind: 'start' });
    expect(parseRoute('#/p/00ff')).toEqual({ kind: 'project', id: '00ff' });
    expect(parseRoute('')).toEqual({ kind: 'none' });
    expect(parseRoute('#/p/../x')).toEqual({ kind: 'none' });
  });
});

describe('applyTemplate', () => {
  it('replaces the open piece in one undo step and undo brings it back', async () => {
    const store = await fresh('tpl-undo');
    await store.group('Starter', starterCommands(counterIds()));
    const before = store.getSnapshot();
    const depth = before.undoDepth;
    const ok = await newActions(store).applyTemplate('cinematic');
    expect(ok).toBe(true);
    const after = store.getSnapshot();
    expect(after.project.tracks.map((t) => t.name)).toEqual(['Strings pad', 'Low piano', 'Pulse', 'Boom']);
    expect(after.undoDepth).toBe(depth + 1);
    expect(after.selectedRegion).not.toBeNull();
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(before.stateHash);
    store.dispose();
  });
});

describe('next action rules', () => {
  it('asks for a start when there are no tracks', async () => {
    const store = await fresh('rule-empty');
    expect(nextAction(store.getSnapshot().project, null, false)?.id).toBe('start');
    store.dispose();
  });

  it('suggests drawing a note in an empty clip, then repeating the lone clip', async () => {
    const store = await fresh('rule-blank');
    const actions = newActions(store);
    await actions.applyTemplate('blank');
    const snap = store.getSnapshot();
    expect(nextAction(snap.project, snap.selectedRegion, false)?.id).toBe('empty-clip');
    if (snap.selectedRegion) await actions.addNoteAt(snap.selectedRegion, 60, 0, 960);
    const next = store.getSnapshot();
    expect(nextAction(next.project, next.selectedRegion, false)?.id).toBe('repeat');
    store.dispose();
  });

  it('suggests a sound while a track still has the basic one, and stays quiet for a finished template', async () => {
    const store = await fresh('rule-sound');
    await store.group('Starter', starterCommands(counterIds()));
    const plain = store.getSnapshot();
    expect(nextAction(plain.project, plain.selectedRegion, false)?.id).toBe('pick-sound');
    await newActions(store).applyTemplate('cinematic');
    const done = store.getSnapshot();
    expect(nextAction(done.project, done.selectedRegion, false)).toBeNull();
    store.dispose();
  });
});
