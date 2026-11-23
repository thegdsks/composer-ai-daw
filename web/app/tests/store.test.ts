import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { newId } from '../src/ids';
import { addNote, moveNotes } from '../src/state/commands';
import { findRegion, projectEnd, sortedNotes, Store } from '../src/state/store';
import { counterIds, newBackend } from './helpers';

async function seeded(): Promise<Store> {
  const store = new Store(newBackend());
  await store.refresh();
  await store.group('starter', starterCommands(counterIds()));
  return store;
}

describe('Store', () => {
  it('selects the first track and region after the first state', async () => {
    const store = await seeded();
    const snap = store.getSnapshot();
    expect(snap.project.tracks.map((t) => t.name)).toEqual(['Pad', 'Bass']);
    expect(snap.selectedTrack).toBe(snap.project.tracks[0]?.id);
    const found = findRegion(snap.project, snap.selectedRegion);
    expect(found?.region.length).toBe(15360);
    expect(projectEnd(snap.project)).toBe(15360);
    expect(snap.undoDepth).toBe(1);
  });

  it('adds a note through a command, then undoes and redoes it', async () => {
    const store = await seeded();
    const before = store.getSnapshot();
    const region = findRegion(before.project, before.selectedRegion)!.region;
    const count = sortedNotes(region).length;
    const id = newId();
    const r = await store.apply(addNote(id, region.id, 72, 480, 240));
    expect(r.ok).toBe(true);
    const mid = store.getSnapshot();
    const notes = sortedNotes(findRegion(mid.project, mid.selectedRegion)!.region);
    expect(notes).toHaveLength(count + 1);
    expect(notes.find((n) => n.id === id)).toMatchObject({ pitch: 72, start: 480, duration: 240 });
    expect(mid.undoDepth).toBe(2);

    await store.undo();
    const undone = store.getSnapshot();
    expect(undone.stateHash).toBe(before.stateHash);
    expect(sortedNotes(findRegion(undone.project, undone.selectedRegion)!.region)).toHaveLength(count);
    expect(undone.redoDepth).toBe(1);

    await store.redo();
    expect(store.getSnapshot().stateHash).toBe(mid.stateHash);
  });

  it('moves notes as one undo step', async () => {
    const store = await seeded();
    const snap = store.getSnapshot();
    const region = findRegion(snap.project, snap.selectedRegion)!.region;
    const some = sortedNotes(region).slice(0, 3);
    await store.group('Move notes', moveNotes(region, some, 240, 1));
    expect(store.getSnapshot().undoDepth).toBe(snap.undoDepth + 1);
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(snap.stateHash);
  });

  it('drops selected notes that no longer exist', async () => {
    const store = await seeded();
    const snap = store.getSnapshot();
    const region = findRegion(snap.project, snap.selectedRegion)!.region;
    const first = sortedNotes(region)[0]!;
    store.selectNotes([first.id]);
    await store.apply({ command: 'delete_note', region: region.id, note: first.id });
    expect(store.getSnapshot().selectedNotes.size).toBe(0);
  });

  it('keeps the error text when a command is rejected', async () => {
    const store = await seeded();
    const r = await store.apply({ command: 'delete_track', track: 'ab'.repeat(16) });
    expect(r.ok).toBe(false);
    expect(store.getSnapshot().error).toContain('not_found');
  });
});
