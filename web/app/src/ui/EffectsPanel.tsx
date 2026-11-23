import { useEffect, useRef, useState } from 'react';
import { config } from '../config';
import { PlusIcon } from '../icons';
import { DeviceIcon } from '../icons/devices';
import type { Effect, EffectOwner } from '../model';
import { chainOf, DEVICES, dropIndex, ownerKey, ownerRefusal } from '../state/effectsModel';
import { ContextMenu, useMenuTrigger } from './ContextMenu';
import { EffectCard, type EffectFocus } from './EffectCard';
import { useSession, useSnapshot } from './hooks';
import type { MenuItem } from './menuModel';

const MASTER = 'master';
const noMenu = (): null => null;

/** Effects tab: the chain of the selected track or of the master, one card per device, plus the Add effect menu. */
export function EffectsPanel() {
  const { project, selectedTrack } = useSnapshot();
  const { actions, store } = useSession();
  // Master stays chosen until a track is picked again, here or in the arrangement.
  const [masterOn, setMasterOn] = useState(false);
  const [seenTrack, setSeenTrack] = useState(selectedTrack);
  if (seenTrack !== selectedTrack) {
    setSeenTrack(selectedTrack);
    setMasterOn(false);
  }
  const trackId = selectedTrack ?? project.tracks[0]?.id ?? null;
  const owner: EffectOwner | null = masterOn ? { kind: 'master' } : trackId ? { kind: 'track', track: trackId } : null;
  const chain: readonly Effect[] = owner ? chainOf(project, owner) : [];
  const refusal = owner ? ownerRefusal(project, owner) : 'Add a track first.';
  const full = chain.length >= config.effectChainMax;

  // After a move the card is re-inserted, so put focus back on the control that was used.
  const refocus = useRef<{ id: string; test: string } | null>(null);
  useEffect(() => {
    const want = refocus.current;
    if (!want) return;
    refocus.current = null;
    document.querySelector<HTMLElement>(`[data-effect="${want.id}"] [data-testid="${want.test}"]`)?.focus();
  }, [project]);

  const onMove = (effectId: string, to: number, focus: EffectFocus): void => {
    refocus.current = { id: effectId, test: focus };
    void actions.moveEffect(effectId, to);
  };

  const items = (): MenuItem[] =>
    DEVICES.map((d) => ({
      id: d.kind,
      label: d.name,
      hint: d.blurb,
      run: () => {
        if (owner) void actions.addEffect(owner, d.kind);
      },
    }));
  // Opened from the button only, so the right click builder has nothing to offer.
  const trigger = useMenuTrigger(noMenu);

  const options = [
    ...project.tracks.map((t) => ({ value: t.id, label: t.name })),
    { value: MASTER, label: 'Master (all tracks)' },
  ];
  const selected = masterOn ? MASTER : (trackId ?? '');

  return (
    <div data-testid="effects-panel" className="flex min-h-0 flex-1 flex-col gap-2 p-3">
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm text-ink-dim">
          Effects for
          <select
            data-testid="effects-owner"
            className="h-8 rounded border border-line bg-surface px-2 text-sm text-ink"
            value={selected}
            onChange={(e) => {
              if (e.target.value === MASTER) setMasterOn(true);
              else {
                setMasterOn(false);
                store.selectTrack(e.target.value);
              }
            }}
          >
            {options.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          data-testid="effects-add"
          aria-haspopup="menu"
          aria-disabled={refusal !== null || full}
          className="inline-flex h-8 items-center gap-1.5 rounded border border-line px-3 text-sm font-medium text-ink hover:bg-panel-2 aria-disabled:cursor-not-allowed aria-disabled:text-ink-dim"
          onClick={(e) => {
            if (refusal !== null || full) return;
            const box = e.currentTarget.getBoundingClientRect();
            trigger.openAt({ label: 'Add effect', items: items(), x: box.left, y: box.bottom });
          }}
        >
          <PlusIcon size={16} />
          Add effect
        </button>
        <span className="text-xs text-ink-dim" aria-live="polite">
          {refusal ?? (full ? `A chain holds at most ${config.effectChainMax} effects.` : '')}
        </span>
      </div>
      {chain.length === 0 && refusal === null ? (
        <div data-testid="effects-empty" className="flex flex-col gap-2 text-sm text-ink-dim">
          <p>{owner?.kind === 'master' ? 'Nothing on the master yet.' : 'No effects on this track yet.'} Pick one to start:</p>
          <ul className="flex flex-wrap gap-2">
            {DEVICES.map((d) => (
              <li key={d.kind}>
                <button
                  type="button"
                  data-testid={`effects-quick-${d.kind}`}
                  title={d.blurb}
                  className="inline-flex h-8 items-center gap-1.5 rounded border border-line px-3 text-sm text-ink hover:bg-panel-2"
                  onClick={() => owner && void actions.addEffect(owner, d.kind)}
                >
                  <DeviceIcon kind={d.kind} />
                  {d.name}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <ol
          key={owner ? ownerKey(owner) : 'none'}
          aria-label="Effect chain, first to last"
          data-testid="effect-chain"
          className="flex min-h-0 flex-1 items-start gap-2 overflow-x-auto pb-2"
        >
          {chain.map((e, i) => (
            <EffectCard
              key={e.id}
              effect={e}
              index={i}
              count={chain.length}
              actions={actions}
              onMove={onMove}
              dropTarget={(dragged, onto) => dropIndex(chain, dragged, onto)}
            />
          ))}
        </ol>
      )}
      <ContextMenu request={trigger.request} onClose={trigger.close} />
    </div>
  );
}
