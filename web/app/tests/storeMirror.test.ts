// Store with the change feed mirror (U27): above the note limit an edit no longer refetches the
// project, a random session ends on the backend's state, and a deliberate mismatch is caught.
import { describe, expect, it } from 'vitest';
import type { Backend, Reply, StateSnapshot } from '../src/backend/types';
import type { Command, Project } from '../src/model';
import { Store } from '../src/state/store';
import { counterIds, newBackend } from './helpers';
import { randomCommand, rng } from './randomSession';

/** Counts full state fetches, the work the mirror is meant to remove. */
function counting(backend: Backend): { fetches: () => number } {
  let n = 0;
  const original = backend.state.bind(backend);
  backend.state = async (): Promise<Reply<StateSnapshot>> => {
    n++;
    return original();
  };
  return { fetches: () => n };
}

function bigProject(id: () => string, notes: number): Command[] {
  const track = id();
  const region = id();
  const out: Command[] = [
    { command: 'add_track', id: track, name: 'Big', kind: 'instrument', index: null },
    { command: 'add_region', id: region, track, start: 0, length: 960 * 4 * 64, kind: 'midi' },
  ];
  for (let i = 0; i < notes; i++) {
    out.push({ command: 'add_note', id: id(), region, pitch: 40 + (i % 40), velocity: 90, start: i * 120, duration: 60 });
  }
  return out;
}

type Step = ReturnType<typeof randomCommand>;

async function run(s: Store, step: Step): Promise<void> {
  if (step === 'undo') await s.undo();
  else if (step === 'redo') await s.redo();
  else if (Array.isArray(step)) await s.group('pair', step);
  else await s.apply(step);
}

function sortKeys(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === 'object') {
    return Object.fromEntries(
      Object.entries(v as Record<string, unknown>)
        .sort(([a], [b]) => (a < b ? -1 : 1))
        .map(([k, x]) => [k, sortKeys(x)]),
    );
  }
  return v;
}

describe('Store with the change feed mirror', () => {
  it('does not refetch after an edit above the note limit, and refetches below it', async () => {
    for (const limit of [10, 1000]) {
      const backend = newBackend(`limit${limit}`);
      const seen = counting(backend);
      const store = new Store(backend, { fullRefreshMaxNotes: limit, verifyEvery: 0 });
      await store.refresh();
      const id = counterIds();
      await store.group('seed', bigProject(id, 100));
      const region = store.getSnapshot().selectedRegion as string;
      const before = seen.fetches();
      for (let i = 0; i < 5; i++) await store.apply({ command: 'add_note', id: id(), region, pitch: 60, velocity: 80, start: 5 * 120 * i, duration: 30 });
      await store.undo();
      await store.redo();
      const extra = seen.fetches() - before;
      if (limit === 10) expect(extra, 'mirror path fetches nothing').toBe(0);
      else expect(extra, 'small projects keep the refetch').toBeGreaterThanOrEqual(7);
      const truth = await backend.state();
      if (!truth.ok) throw new Error('state');
      expect(store.getSnapshot().stateHash).toBe(truth.value.stateHash);
      expect(JSON.stringify(sortKeys(store.getSnapshot().project))).toBe(JSON.stringify(sortKeys(truth.value.project)));
      store.dispose();
    }
  });

  it('ends a random 400 command session on the same state as a store that refetches', async () => {
    const mirrored = new Store(newBackend('rand-a'), { fullRefreshMaxNotes: -1, verifyEvery: 25 });
    const plain = new Store(newBackend('rand-a'), { fullRefreshMaxNotes: Number.MAX_SAFE_INTEGER });
    await mirrored.refresh();
    await plain.refresh();
    const rand = rng(777);
    const id = counterIds();
    for (let i = 0; i < 400; i++) {
      const step = randomCommand(mirrored.getSnapshot().project as Project, rand, id);
      await run(mirrored, step);
      await run(plain, JSON.parse(JSON.stringify(step)) as Step);
      expect(mirrored.getSnapshot().stateHash, `step ${i}`).toBe(plain.getSnapshot().stateHash);
    }
    const stats = mirrored.mirrorStats();
    expect(stats?.feedsApplied ?? 0).toBeGreaterThan(150);
    expect(stats?.auditFailures).toBe(0);
    expect(stats?.audits ?? 0).toBeGreaterThan(3);
    // Content, not only the hash.
    expect(JSON.stringify(sortKeys(mirrored.getSnapshot().project))).toBe(JSON.stringify(sortKeys(plain.getSnapshot().project)));
    mirrored.dispose();
    plain.dispose();
  });

  it('catches a deliberate mismatch: a tampered mirror is replaced by a full fetch', async () => {
    const backend = newBackend('drift');
    const store = new Store(backend, { fullRefreshMaxNotes: -1, verifyEvery: 3 });
    await store.refresh();
    const id = counterIds();
    const track = id();
    await store.apply({ command: 'add_track', id: track, name: 'A', kind: 'instrument', index: null });
    // Silent drift: the mirror's copy changes while its hash stays, as a missed edit would leave it.
    const t = store.getSnapshot().project.tracks.find((x) => x.id === track);
    if (!t) throw new Error('track');
    t.name = 'drifted';
    await store.apply({ command: 'set_track_volume', track, volume: 900 });
    await store.apply({ command: 'set_track_pan', track, pan: 100 });
    await store.apply({ command: 'set_track_volume', track, volume: 800 });
    // The sampled audit runs after the third feed; give its queued fetch a turn.
    for (let i = 0; i < 40 && (store.mirrorStats()?.auditFailures ?? 0) === 0; i++) await new Promise((r) => setTimeout(r, 5));
    expect(store.mirrorStats()?.auditFailures).toBe(1);
    expect(store.getSnapshot().project.tracks.find((x) => x.id === track)?.name).toBe('A');
    const truth = await backend.state();
    if (!truth.ok) throw new Error('state');
    expect(store.getSnapshot().stateHash).toBe(truth.value.stateHash);
    store.dispose();
  });

  it('falls back to a full fetch when the base hash does not match', async () => {
    const backend = newBackend('base');
    const seen = counting(backend);
    const store = new Store(backend, { fullRefreshMaxNotes: -1, verifyEvery: 0 });
    await store.refresh();
    const id = counterIds();
    await store.apply({ command: 'add_track', id: id(), name: 'A', kind: 'instrument', index: null });
    // An edit made behind the store's back: its next feed starts from a hash the mirror lacks.
    await backend.apply({ command: 'add_track', id: id(), name: 'Hidden', kind: 'instrument', index: null });
    const before = seen.fetches();
    await store.apply({ command: 'add_track', id: id(), name: 'C', kind: 'instrument', index: null });
    expect(seen.fetches()).toBeGreaterThan(before);
    expect(store.getSnapshot().project.tracks.map((t) => t.name)).toEqual(['A', 'Hidden', 'C']);
    expect(store.mirrorStats()?.fallbacks).toBe(1);
    store.dispose();
  });
});
