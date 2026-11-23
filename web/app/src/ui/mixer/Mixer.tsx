import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { config } from '../../config';
import { mixOf, type Track } from '../../model';
import { clampDb, dbText, dbToPermille, faderMaxDb, permilleToDb } from '../../state/fader';
import { panText } from '../../state/mix';
import { PlusIcon } from '../../icons';
import { EmptyState } from '../EmptyState';
import { btnSmall } from '../classes';
import { useSession, useSnapshot } from '../hooks';
import { Meter } from './Meter';
import { StripSlider } from './StripSlider';

const toggle =
  'inline-flex size-7 items-center justify-center rounded border border-line text-xs font-semibold motion-safe:transition-colors';

function Strip(props: { track: Track; index: number; showPan: boolean }) {
  const { actions } = useSession();
  const { track } = props;
  const mix = mixOf(track);
  const volumeDb = permilleToDb(mix.volume);
  return (
    <div
      role="group"
      aria-label={`Channel ${props.index + 1}, ${track.name}`}
      data-testid={`strip-${props.index}`}
      className="flex h-full w-24 shrink-0 flex-col items-center gap-1.5 border-r border-line px-2 py-2"
    >
      <p className="m-0 w-full truncate text-center text-[13px] font-medium text-ink" title={track.name}>
        {track.name}
      </p>
      <div className="flex min-h-20 w-full flex-1 items-stretch justify-center gap-2">
        <Meter trackId={track.id} name={track.name} index={props.index} />
        <StripSlider
          vertical
          testId={`fader-${props.index}`}
          label={`Volume of ${track.name}`}
          className="h-full w-6 min-w-0 accent-accent"
          value={volumeDb}
          min={config.faderMinDb}
          max={faderMaxDb()}
          step={config.faderStepDb}
          neutral={0}
          text={dbText}
          same={(db) => dbToPermille(clampDb(db)) === mix.volume}
          onCommit={(db) => actions.setVolume(track.id, dbToPermille(clampDb(db)))}
        />
      </div>
      <output className="text-xs tabular-nums text-ink-dim" aria-hidden="true">
        {dbText(volumeDb)}
      </output>
      {props.showPan && (
        <StripSlider
          testId={`pan-${props.index}`}
          label={`Pan of ${track.name}`}
          className="h-5 w-full accent-accent"
          value={mix.pan}
          min={-config.panMax}
          max={config.panMax}
          step={config.panStep}
          neutral={0}
          text={panText}
          same={(v) => v === mix.pan}
          onCommit={(v) => actions.setPan(track.id, v)}
        />
      )}
      <div className="flex items-center gap-1">
        <button
          type="button"
          className={`${toggle} ${mix.mute ? 'border-accent bg-accent text-accent-ink' : 'bg-panel text-ink hover:bg-panel-2'}`}
          aria-pressed={mix.mute}
          aria-label={`Mute ${track.name}`}
          title="Mute"
          data-testid={`strip-mute-${props.index}`}
          onClick={() => void actions.setMute(track.id, !mix.mute)}
        >
          M
        </button>
        <button
          type="button"
          className={`${toggle} ${mix.solo ? 'border-accent bg-accent text-accent-ink' : 'bg-panel text-ink hover:bg-panel-2'}`}
          aria-pressed={mix.solo}
          aria-label={`Solo ${track.name}`}
          title="Solo"
          data-testid={`strip-solo-${props.index}`}
          onClick={() => void actions.setSolo(track.id, !mix.solo)}
        >
          S
        </button>
      </div>
    </div>
  );
}

/** Strips for the tracks in view only; a spacer on each side keeps the scroll width right. */
function useWindow(count: number) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [view, setView] = useState({ left: 0, width: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = (): void => setView({ left: el.scrollLeft, width: el.clientWidth });
    read();
    const ro = new ResizeObserver(read);
    ro.observe(el);
    el.addEventListener('scroll', read, { passive: true });
    return () => {
      ro.disconnect();
      el.removeEventListener('scroll', read);
    };
  }, []);
  const first = Math.max(0, Math.floor(view.left / config.stripWidth) - config.stripOverscan);
  const last = Math.min(count, Math.ceil((view.left + view.width) / config.stripWidth) + config.stripOverscan);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.setProperty('--strip-lead', `${first * config.stripWidth}px`);
    el.style.setProperty('--strip-trail', `${Math.max(0, count - last) * config.stripWidth}px`);
  }, [first, last, count]);
  return { ref, first, last };
}

/** Mix tab: one strip per track with a dB fader, meter, mute, solo and a disclosed pan. */
export function Mixer() {
  const { project } = useSnapshot();
  const { actions } = useSession();
  const [showPan, setShowPan] = useState(false);
  const tracks = project.tracks;
  const { ref, first, last } = useWindow(tracks.length);

  if (tracks.length === 0) {
    return (
      <EmptyState
        title="No tracks to mix yet"
        hint="Each track gets its own fader here once you add it."
        primary={{ label: 'Add a track', icon: <PlusIcon size={16} />, onClick: () => void actions.addTrack() }}
      />
    );
  }

  return (
    <div className="flex min-h-56 flex-1 flex-col" data-testid="mixer">
      <div className="flex shrink-0 items-center justify-between gap-2 px-3 py-1">
        <p className="m-0 text-xs text-ink-dim">Arrow keys move a fader, Home resets it to 0 dB.</p>
        <button
          type="button"
          className={btnSmall}
          aria-pressed={showPan}
          data-testid="mixer-pan-toggle"
          onClick={() => setShowPan((v) => !v)}
        >
          {showPan ? 'Hide pan' : 'Show pan'}
        </button>
      </div>
      <div
        ref={ref}
        role="group"
        aria-label="Mixer strips"
        data-testid="mixer-strips"
        className="flex min-h-0 flex-1 overflow-x-auto overflow-y-hidden border-t border-line"
      >
        <div className="w-(--strip-lead,0px) shrink-0" aria-hidden="true" />
        {tracks.slice(first, last).map((t, i) => (
          <Strip key={t.id} track={t} index={first + i} showPan={showPan} />
        ))}
        <div className="w-(--strip-trail,0px) shrink-0" aria-hidden="true" />
      </div>
    </div>
  );
}
