// React side of AI assist: the waiting count for badges and the cards for the palette.
import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { config } from '../config';
import {
  candidateRequests,
  cardFromProposal,
  cardFromRecipe,
  type AssistCard,
} from '../state/assist';
import { proposalWatch } from '../state/proposalWatch';
import { useSession } from './hooks';

/** Proposals from AI clients that wait for a person. */
export function useProposalCount(): number {
  const { store } = useSession();
  const w = proposalWatch(store);
  return useSyncExternalStore(w.subscribe, w.getCount, () => 0);
}

/** Cards for the selection at the moment the palette opened: waiting proposals first, then local recipes. */
export function useAssistCards(): { cards: AssistCard[]; loading: boolean; reload(): void } {
  const { store } = useSession();
  const [cards, setCards] = useState<AssistCard[]>([]);
  const [loading, setLoading] = useState(true);
  const [round, setRound] = useState(0);

  useEffect(() => {
    let live = true;
    const backend = store.backend;
    const snap = store.getSnapshot();
    void (async () => {
      const out: AssistCard[] = [];
      const listed = await backend.proposalList?.('pending');
      if (listed?.ok) for (const p of listed.value.proposals) out.push(cardFromProposal(p));
      const recipes = await backend.recipesList?.();
      if (recipes?.ok && backend.recipePlan) {
        const requests = candidateRequests(
          { track: snap.selectedTrack, region: snap.selectedRegion, tracks: snap.project.tracks.length },
          recipes.value,
        );
        const plans = await Promise.all(requests.map((q) => backend.recipePlan!(q)));
        plans.forEach((plan, i) => {
          const request = requests[i];
          const info = recipes.value.find((r) => r.name === request?.recipe);
          // A recipe that cannot run on this selection, or would change nothing, is not worth a card.
          if (request && info && plan.ok && plan.value.wouldChange && plan.value.applies) {
            out.push(cardFromRecipe(request, info, plan.value));
          }
        });
      }
      if (!live) return;
      setCards(out.slice(0, Math.max(config.assistMaxCards, out.filter((c) => c.origin === 'proposal').length)));
      setLoading(false);
      void proposalWatch(store).refresh();
    })();
    return () => {
      live = false;
    };
  }, [store, round]);

  const reload = useCallback(() => setRound((n) => n + 1), []);
  return { cards, loading, reload };
}
