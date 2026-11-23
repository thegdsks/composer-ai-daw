import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import { duplicateClip, placeClip, splitClip } from '../src/state/commands';
import { findRegion, Store } from '../src/state/store';
import { notesOf } from '../src/model';
import { counterIds, newActions, newBackend } from './helpers';

const BAR = 3840;

async function setup() {
  const store = new Store(newBackend());
  await store.refresh();
  await store.group('starter', starterCommands(counterIds()));
  // History starts over so each test counts only its own steps.
  const s = await store.backend.state();
  if (s.ok) await store.load(s.value.project);
  await store.refresh();
  const actions = newActions(store);
  const [pad, bass] = store.getSnapshot().project.tracks;
  const padClip = Object.values(pad!.regions)[0]!;
  const bassClip = Object.values(bass!.regions)[0]!;
  return { store, actions, pad: pad!, bass: bass!, padClip, bassClip };
}

describe('clip edit builders', () => {
  it('splits only inside the clip and pairs one id with each crossing note', () => {
    const r = {
      id: 'r',
      start: BAR,
      length: BAR * 2,
      content: {
        midi: {
          notes: {
            a: { id: 'a', pitch: 60, velocity: 90, start: 0, duration: 100 },
            b: { id: 'b', pitch: 62, velocity: 90, start: 3000, duration: 1500 },
          },
        },
      },
    };
    const ids = counterIds();
    expect(splitClip(r, BAR, ids)).toBeNull();
    expect(splitClip(r, BAR * 3, ids)).toBeNull();
    const cmd = splitClip(r, BAR + 4000, ids);
    expect(cmd).toMatchObject({ command: 'split_region', at: BAR + 4000 });
    expect(cmd?.command === 'split_region' && cmd.note_ids).toHaveLength(1);
  });

  it('copies one id per note and skips unchanged fields when placing', () => {
    const r = { id: 'r', start: 0, length: BAR, content: { midi: { notes: { a: { id: 'a', pitch: 60, velocity: 90, start: 0, duration: 10 } } } } };
    const cmd = duplicateClip(r, BAR, null, counterIds());
    expect(cmd.command === 'duplicate_region' && cmd.note_ids).toHaveLength(1);
    expect(placeClip(r, 't1', 't1', 0)).toEqual([]);
    expect(placeClip(r, 't1', 't2', BAR).map((c) => c.command)).toEqual(['move_region_to_track', 'move_region']);
  });
});

describe('clip edits through the store', () => {
  it('moves a clip to another lane and later in time as one undo step', async () => {
    const { store, actions, bass, padClip } = await setup();
    const before = store.getSnapshot();
    await actions.moveClips([{ regionId: padClip.id, trackId: bass.id, start: BAR * 5 }], padClip.id);
    const after = store.getSnapshot();
    expect(after.undoDepth).toBe(before.undoDepth + 1);
    const found = findRegion(after.project, padClip.id);
    expect(found?.track.id).toBe(bass.id);
    expect(found?.region.start).toBe(BAR * 5);
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(before.stateHash);
  });

  it('makes a new track when dropped below the last lane, in one step', async () => {
    const { store, actions, padClip } = await setup();
    const before = store.getSnapshot();
    await actions.moveClips([{ regionId: padClip.id, trackId: null, start: 0 }], padClip.id);
    const after = store.getSnapshot();
    expect(after.project.tracks).toHaveLength(3);
    expect(findRegion(after.project, padClip.id)?.track.id).toBe(after.project.tracks[2]?.id);
    expect(after.undoDepth).toBe(before.undoDepth + 1);
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(before.stateHash);
  });

  it('copies clips with their notes in one step and selects the copy', async () => {
    const { store, actions, padClip } = await setup();
    const before = store.getSnapshot();
    await actions.copyClips([{ regionId: padClip.id, trackId: null, start: BAR * 8 }], padClip.id);
    const after = store.getSnapshot();
    expect(after.undoDepth).toBe(before.undoDepth + 1);
    const copy = findRegion(after.project, after.selectedRegion)!;
    expect(copy.region.id).not.toBe(padClip.id);
    expect(copy.region.start).toBe(BAR * 8);
    expect(notesOf(copy.region)).toHaveLength(notesOf(padClip).length);
    expect(findRegion(after.project, padClip.id)).not.toBeNull();
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(before.stateHash);
  });

  it('splits at a tick, keeps every note and undoes in one step', async () => {
    const { store, actions, padClip } = await setup();
    const before = store.getSnapshot();
    // Mid bar 2 cuts through the second chord.
    const cut = BAR + 1000;
    expect(await actions.splitClips([padClip.id], cut)).toBe(1);
    const after = store.getSnapshot();
    expect(after.undoDepth).toBe(before.undoDepth + 1);
    const pad = after.project.tracks[0]!;
    const parts = Object.values(pad.regions).sort((a, b) => a.start - b.start);
    expect(parts.map((p) => [p.start, p.length])).toEqual([
      [0, cut],
      [cut, padClip.length - cut],
    ]);
    expect(parts.reduce((n, p) => n + notesOf(p).length, 0)).toBe(notesOf(padClip).length + 3);
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(before.stateHash);
  });

  it('refuses a cut outside the clip without changing history', async () => {
    const { store, actions, padClip } = await setup();
    const depth = store.getSnapshot().undoDepth;
    expect(await actions.splitClips([padClip.id], padClip.start + padClip.length + 10)).toBe(0);
    expect(store.getSnapshot().undoDepth).toBe(depth);
  });

  it('deletes several selected clips in one step', async () => {
    const { store, actions, padClip, bassClip } = await setup();
    const before = store.getSnapshot();
    await actions.deleteClips([padClip.id, bassClip.id]);
    expect(store.getSnapshot().undoDepth).toBe(before.undoDepth + 1);
    expect(store.getSnapshot().project.tracks.every((t) => Object.keys(t.regions).length === 0)).toBe(true);
    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(before.stateHash);
  });
});

describe('clip selection', () => {
  it('keeps a set with one primary and drops ids that disappear', async () => {
    const { store, actions, pad, padClip, bassClip } = await setup();
    store.selectRegions(pad.id, padClip.id, [padClip.id, bassClip.id]);
    expect(store.getSnapshot().selectedRegions.size).toBe(2);
    await actions.deleteClips([bassClip.id]);
    const snap = store.getSnapshot();
    expect([...snap.selectedRegions]).toEqual([padClip.id]);
    expect(snap.selectedRegion).toBe(padClip.id);
  });

  it('keeps the moved clip selected, not a neighbour, after undo returns it to its old track', async () => {
    const { store, actions, bass, padClip } = await setup();
    await actions.moveClips([{ regionId: padClip.id, trackId: bass.id, start: 0 }], padClip.id);
    await store.undo();
    const snap = store.getSnapshot();
    expect([...snap.selectedRegions]).toEqual([padClip.id]);
    expect(snap.selectedTrack).toBe(snap.project.tracks[0]?.id);
  });

  it('announces navigation through say', async () => {
    const { store } = await setup();
    const heard: string[] = [];
    store.onAnnounce((t) => heard.push(t));
    store.say('Pad, bars 1 to 4');
    expect(heard).toEqual(['Pad, bars 1 to 4']);
  });
});
