import { useState, useSyncExternalStore } from 'react';
import { config } from '../config';
import { XIcon } from '../icons';
import { findRegion } from '../state/store';
import { nextAction, type NextAction as NextActionRule, type NextActionId } from '../nextAction';
import { getPrefs, setPrefs, subscribePrefs } from '../prefs';
import { btnPrimary, compact } from './classes';
import type { DockTab } from './Dock';
import { useSession, useSnapshot } from './hooks';
import { useProposalCount } from './useAssist';

/** One small suggestion under the top bar. It hides after a few dismissals and while the start sheet is open. */
export function NextAction(props: { hidden: boolean; onStart(): void; onTab(tab: DockTab): void; onAssist(): void }) {
  const { actions, store, debug } = useSession();
  const snap = useSnapshot();
  const prefs = useSyncExternalStore(subscribePrefs, getPrefs);
  const [dismissed, setDismissed] = useState<ReadonlySet<NextActionId>>(new Set());
  const waiting = useProposalCount();
  if (props.hidden || prefs.chipDismissals >= config.nextActionMaxDismissals) return null;
  // Suggestions from AI clients come first: they are the only thing here that someone else made.
  const rule: NextActionRule | null = waiting > 0
    ? { id: 'assist', text: waiting === 1 ? 'One suggestion is waiting for you.' : `${waiting} suggestions are waiting for you.`, button: 'Review' }
    : nextAction(snap.project, snap.selectedRegion, debug.lastExport !== null);
  if (!rule || dismissed.has(rule.id)) return null;

  const run = (): void => {
    const found = findRegion(snap.project, snap.selectedRegion);
    switch (rule.id) {
      case 'assist':
        props.onAssist();
        break;
      case 'start':
        props.onStart();
        break;
      case 'empty-clip':
        if (found) void actions.addNoteAt(found.region.id, config.keyboardBaseNote, 0, config.ticksPerQuarter);
        break;
      case 'pick-sound': {
        const plain = snap.project.tracks.find((t) => !t.sound);
        if (plain) store.selectTrack(plain.id);
        props.onTab('sound');
        break;
      }
      case 'repeat':
        if (found) void actions.copyClips([{ regionId: found.region.id, trackId: null, start: found.region.start + found.region.length }], found.region.id);
        break;
      case 'export':
        void actions.exportWav();
        break;
    }
  };

  return (
    <div
      role="region"
      aria-label="Suggestion"
      data-testid="next-action"
      data-rule={rule.id}
      className="flex items-center gap-2 border-b border-line bg-panel-2 px-3 py-0.5 text-sm"
    >
      <span className="min-w-0 truncate text-ink-dim">{rule.text}</span>
      <button type="button" data-primary="true" className={`${compact(btnPrimary)} shrink-0`} onClick={run}>
        {rule.button}
      </button>
      <button
        type="button"
        className="inline-flex size-8 shrink-0 items-center justify-center rounded-md text-ink-dim hover:bg-panel hover:text-ink"
        aria-label="Hide this suggestion"
        onClick={() => {
          setDismissed(new Set([...dismissed, rule.id]));
          setPrefs({ chipDismissals: prefs.chipDismissals + 1 });
        }}
      >
        <XIcon size={16} />
      </button>
    </div>
  );
}
