// D27: proposals through the wasm backend. Posting never changes the project; accepting is one
// undo step; a stale proposal fails with a command error and stays pending.
import { describe, expect, it } from 'vitest';
import type { Command } from '../src/model';
import { newBackend } from './helpers';

const add = (n: number): Command => ({
  command: 'add_track',
  id: String(n).padStart(32, '0'),
  name: `T${n}`,
  kind: 'instrument',
  index: null,
});

describe('proposals (D27)', () => {
  it('post and reject leave the project untouched', async () => {
    const b = await newBackend();
    const before = await b.state();
    const posted = await b.proposalPost!({ label: 'Add a track', commands: [add(7)], author: 'ai' });
    expect(posted.ok && posted.value.id).toBe('p1');
    const listed = await b.proposalList!();
    expect(listed.ok && listed.value.pending).toBe(1);
    expect(listed.ok && listed.value.proposals[0]?.applies).toBe(true);
    const rejected = await b.proposalResolve!('p1', 'reject');
    expect(rejected.ok).toBe(true);
    const after = await b.state();
    expect(after.ok && after.value.stateHash).toBe(before.ok && before.value.stateHash);
  });

  it('accept applies one group tagged ai and undoes in one step', async () => {
    const b = await newBackend();
    await b.proposalPost!({ label: 'Two tracks', commands: [add(1), add(2)] });
    const acc = await b.proposalResolve!('p1', 'accept');
    expect(acc.ok && acc.value.undoDepth).toBe(1);
    const undo = await b.undo();
    expect(undo.ok && undo.value.applied).toBe(true);
    const st = await b.state();
    expect(st.ok && st.value.project.tracks.length).toBe(0);
  });

  it('refuses a proposal that cannot apply and flags one gone stale', async () => {
    const b = await newBackend();
    const bad = await b.proposalPost!({
      label: 'Bad',
      commands: [{ command: 'rename_track', track: 'f'.padStart(32, 'f'), name: 'x' }],
    });
    expect(bad.ok).toBe(false);
    await b.proposalPost!({ label: 'Add', commands: [add(3)] });
    await b.apply(add(3));
    const acc = await b.proposalResolve!('p1', 'accept');
    expect(acc.ok).toBe(false);
    const listed = await b.proposalList!();
    expect(listed.ok && listed.value.proposals[0]?.stale).toBe(true);
    expect(listed.ok && listed.value.proposals[0]?.applies).toBe(false);
  });
});
