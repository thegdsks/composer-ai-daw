// Preview, accept and reject for assist cards. A preview is an open group (D32): the commands
// run live so the screen and the sound show them, and a rollback restores the exact state hash.
// Accepting always rolls a preview back first, then lets the backend apply the card as ONE step.
import { config } from '../config';
import type { Backend, Summary } from '../backend/types';
import { plainError } from './errors';
import type { AssistCard } from './assist';
import type { Store } from './store';

export type RunResult = { ok: true; summary: Summary } | { ok: false; message: string };

export class AssistRun {
  private token: string | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly store: Store) {}

  private get backend(): Backend {
    return this.store.backend;
  }

  get previewing(): boolean {
    return this.token !== null;
  }

  /** Runs the card's commands inside an open group so they show live; nothing is kept. */
  async preview(card: AssistCard): Promise<{ ok: true } | { ok: false; message: string }> {
    if (this.token) return { ok: true };
    const begun = await this.backend.groupBegin(`Preview: ${card.label}`, card.origin === 'proposal' ? 'ai' : 'recipe');
    if (!begun.ok) return { ok: false, message: plainError(begun.error) };
    this.token = begun.value.token;
    for (const command of card.commands) {
      const r = await this.backend.apply(command);
      if (!r.ok) {
        await this.discard();
        return { ok: false, message: plainError(r.error) };
      }
    }
    await this.store.refresh();
    // The wasm session has no clock, so the page owns the timeout (the daemon has its own).
    this.timer = setTimeout(() => void this.discard(), config.assistPreviewMaxMs);
    return { ok: true };
  }

  /** Rolls a preview back. Safe to call when none is open. */
  async discard(): Promise<void> {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    const token = this.token;
    this.token = null;
    if (!token) return;
    await this.backend.groupRollback(token);
    await this.store.refresh();
  }

  /** Accepts the card as one undo step tagged by its source (proposals: AI). */
  async accept(card: AssistCard): Promise<RunResult> {
    await this.discard();
    let r;
    if (card.proposalId && this.backend.proposalResolve) {
      r = await this.backend.proposalResolve(card.proposalId, 'accept');
    } else if (card.request && this.backend.recipeApply) {
      r = await this.backend.recipeApply(card.request);
    } else {
      return { ok: false, message: 'This backend cannot apply that.' };
    }
    const done = await this.store.settle(r, card.label);
    return done.ok ? { ok: true, summary: done.value } : { ok: false, message: plainError(done.error) };
  }

  /** Drops a proposal; the project is untouched. */
  async reject(card: AssistCard): Promise<RunResult> {
    await this.discard();
    if (!card.proposalId || !this.backend.proposalResolve) return { ok: false, message: 'Only a proposal can be rejected.' };
    const r = await this.backend.proposalResolve(card.proposalId, 'reject');
    return r.ok ? { ok: true, summary: r.value } : { ok: false, message: plainError(r.error) };
  }
}
