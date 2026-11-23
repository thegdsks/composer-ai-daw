// How many proposals from AI clients wait for a person. One shared poll per store, started by
// the first subscriber and stopped by the last, paused while the tab is hidden. The daemon also
// pushes proposals.changed, but polling keeps both backends on one path.
import { config } from '../config';
import type { Store } from './store';

class ProposalWatch {
  private count = 0;
  private readonly listeners = new Set<() => void>();
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(private readonly store: Store) {}

  getCount = (): number => this.count;

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    if (this.listeners.size === 1) this.start();
    return () => {
      this.listeners.delete(l);
      if (this.listeners.size === 0) this.stop();
    };
  };

  /** Reads the waiting count now; called after the person resolves a proposal. */
  async refresh(): Promise<void> {
    const list = this.store.backend.proposalList;
    if (!list) return;
    const r = await list.call(this.store.backend, 'pending');
    if (!r.ok || r.value.pending === this.count) return;
    this.count = r.value.pending;
    for (const l of this.listeners) l();
  }

  private start(): void {
    void this.refresh();
    this.timer = setInterval(() => {
      if (document.visibilityState === 'visible') void this.refresh();
    }, config.assistPollMs);
  }

  private stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }
}

const watches = new WeakMap<Store, ProposalWatch>();

export function proposalWatch(store: Store): ProposalWatch {
  let w = watches.get(store);
  if (!w) {
    w = new ProposalWatch(store);
    watches.set(store, w);
  }
  return w;
}
