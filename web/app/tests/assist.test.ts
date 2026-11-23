// AI assist: the plain summary, the card rules, and preview, accept, reject and stale handling
// against the real wasm backend. There is no model in the app; proposals are posted by a test.
import { describe, expect, it } from 'vitest';
import { starterCommands } from '../src/fixtures';
import type { Command } from '../src/model';
import {
  candidateRequests,
  cardFromProposal,
  cardFromRecipe,
  describeChanges,
  type AssistCard,
} from '../src/state/assist';
import { AssistRun } from '../src/state/assistRun';
import { Store } from '../src/state/store';
import { counterIds, newBackend } from './helpers';

const add = (n: number): Command => ({ command: 'add_track', id: `a${String(n).padStart(31, '0')}`, name: `T${n}`, kind: 'instrument', index: null });

async function seeded(): Promise<Store> {
  const store = new Store(newBackend());
  await store.refresh();
  await store.group('starter', starterCommands(counterIds()));
  return store;
}

async function firstProposalCard(store: Store): Promise<AssistCard> {
  const list = await store.backend.proposalList!('pending');
  if (!list.ok || !list.value.proposals[0]) throw new Error('no proposal');
  return cardFromProposal(list.value.proposals[0]);
}

describe('describeChanges', () => {
  it('counts what a command list does in plain sentences', () => {
    const lines = describeChanges([
      add(1),
      add(2),
      { command: 'add_notes', region: 'r', notes: [1, 2, 3].map((i) => ({ id: String(i), pitch: 60, velocity: 80, start: 0, duration: 10 })) },
      { command: 'set_tempo', tick: 0, mbpm: 90000 },
    ]);
    expect(lines).toEqual(['Adds 2 tracks.', 'Adds 3 notes.', 'Changes the tempo (1 change).']);
  });

  it('never uses the banned jargon words', () => {
    const all = describeChanges([{ command: 'set_notes_velocity', region: 'r', notes: ['a'], value: 5, ramp: null }]).join(' ');
    expect(all).not.toMatch(/velocity|region|quantize/i);
  });
});

describe('candidateRequests', () => {
  it('offers calm sections for an empty project and selection recipes otherwise', () => {
    const recipes = ['calm_sections', 'bass_from_chords', 'soften_velocity'].map((name) => ({ name, title: name, summary: '', scope: '', params: [] }));
    expect(candidateRequests({ track: null, region: null, tracks: 0 }, recipes).map((r) => r.recipe)).toEqual(['calm_sections']);
    expect(candidateRequests({ track: 't', region: 'r', tracks: 2 }, recipes).map((r) => r.recipe)).toEqual(['bass_from_chords', 'soften_velocity']);
  });
});

describe('AssistRun', () => {
  it('preview leaves the hash unchanged after rollback, accept is one step tagged ai, undo restores', async () => {
    const store = await seeded();
    const base = store.getSnapshot();
    await store.backend.proposalPost!({ label: 'Two tracks', commands: [add(1), add(2)], note: 'More room', author: 'agent' });
    const card = await firstProposalCard(store);
    expect(card.changes).toEqual(['Adds 2 tracks.']);
    const run = new AssistRun(store);

    const p = await run.preview(card);
    expect(p.ok).toBe(true);
    expect(store.getSnapshot().project.tracks.length).toBe(base.project.tracks.length + 2);
    await run.discard();
    expect(store.getSnapshot().stateHash).toBe(base.stateHash);
    expect(store.getSnapshot().undoDepth).toBe(base.undoDepth);

    const acc = await run.accept(card);
    expect(acc.ok).toBe(true);
    expect(store.getSnapshot().undoDepth).toBe(base.undoDepth + 1);
    const hist = await store.backend.history(0, 50);
    expect(hist.ok && hist.value.entries.at(-1)?.source).toBe('ai');

    await store.undo();
    expect(store.getSnapshot().stateHash).toBe(base.stateHash);
  });

  it('accepting while a preview is open still adds exactly one step', async () => {
    const store = await seeded();
    const depth = store.getSnapshot().undoDepth;
    await store.backend.proposalPost!({ label: 'One track', commands: [add(5)] });
    const card = await firstProposalCard(store);
    const run = new AssistRun(store);
    await run.preview(card);
    const acc = await run.accept(card);
    expect(acc.ok).toBe(true);
    expect(store.getSnapshot().undoDepth).toBe(depth + 1);
    expect(store.getSnapshot().project.tracks.filter((t) => t.name === 'T5').length).toBe(1);
  });

  it('reject keeps the project untouched and clears the proposal', async () => {
    const store = await seeded();
    const base = store.getSnapshot().stateHash;
    await store.backend.proposalPost!({ label: 'One track', commands: [add(6)] });
    const card = await firstProposalCard(store);
    const r = await new AssistRun(store).reject(card);
    expect(r.ok).toBe(true);
    expect(store.getSnapshot().stateHash).toBe(base);
    const left = await store.backend.proposalList!('pending');
    expect(left.ok && left.value.pending).toBe(0);
  });

  it('flags a stale proposal that no longer fits and refuses to accept it', async () => {
    const store = await seeded();
    await store.backend.proposalPost!({ label: 'One track', commands: [add(7)] });
    await store.apply(add(7));
    const card = await firstProposalCard(store);
    expect(card.stale).toBe(true);
    expect(card.applies).toBe(false);
    const before = store.getSnapshot().stateHash;
    const r = await new AssistRun(store).accept(card);
    expect(r.ok).toBe(false);
    expect(store.getSnapshot().stateHash).toBe(before);
    const still = await store.backend.proposalList!('pending');
    expect(still.ok && still.value.pending).toBe(1);
  });

  it('a recipe card previews without a change and applies as one step tagged recipe', async () => {
    const store = await seeded();
    const snap = store.getSnapshot();
    const request = { recipe: 'soften_velocity', params: { ceiling: 1 }, scope: { region: snap.selectedRegion ?? undefined } };
    const info = { name: 'soften_velocity', title: 'Soften velocity', summary: 'Pulls loud notes down.', scope: '', params: [] };
    const plan = await store.backend.recipePlan!(request);
    if (!plan.ok) throw new Error(plan.error.message);
    const card = cardFromRecipe(request, info, plan.value);
    expect(card.label).toBe('Soften loudness');
    expect(card.rationale).not.toMatch(/velocity/i);
    const run = new AssistRun(store);
    await run.preview(card);
    await run.discard();
    expect(store.getSnapshot().stateHash).toBe(snap.stateHash);
    const acc = await run.accept(card);
    expect(acc.ok).toBe(true);
    expect(store.getSnapshot().undoDepth).toBe(snap.undoDepth + 1);
    const hist = await store.backend.history(0, 50);
    expect(hist.ok && hist.value.entries.at(-1)?.source).toBe('recipe');
  });
});
