import { InstrumentGlyph } from '../icons/instruments';
import { config } from '../config';
import { SearchIcon } from '../icons';
import { ChevronIcon } from '../icons/library';
import { mixOf, type Track } from '../model';
import { panText, volumeText } from '../state/mix';
import {
  categoryName,
  instrumentLabel,
  neighbourSound,
  reverbSendOf,
  soundLabel,
  trackSendText,
} from '../state/soundPanel';
import { btn } from './classes';
import { useSession, useSnapshot } from './hooks';
import { MixSlider } from './MixSlider';
import { DOT } from './SoundBrowser';

const toggle =
  'inline-flex h-9 min-w-9 items-center justify-center rounded-md border px-3 text-sm font-semibold motion-safe:transition-colors';
const slider = 'h-6 min-w-0 flex-1 accent-accent';

function Row(props: { label: string; value: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-3">
      <span className="w-24 shrink-0 text-[13px] text-ink-dim">{props.label}</span>
      {props.children}
      <output className="w-24 shrink-0 text-right text-xs tabular-nums text-ink-dim" aria-hidden="true">
        {props.value}
      </output>
    </div>
  );
}

function Panel({ track }: { track: Track }) {
  const { actions, audio, sounds } = useSession();
  const entries = sounds.catalogue?.entries ?? [];
  const mix = mixOf(track);
  const send = reverbSendOf(track);
  const category = categoryName(track, entries);

  const step = (dir: 1 | -1): void => {
    const next = neighbourSound(entries, track.sound ?? null, dir);
    if (!next) return;
    void actions.setSound(track.id, next.id).then((changed) => {
      if (changed) void audio.preview(next.id);
    });
  };
  const canStep = neighbourSound(entries, track.sound ?? null, 1) !== null;
  const replace = (): void => {
    const el = document.querySelector<HTMLInputElement>('[data-testid=sound-search]');
    el?.scrollIntoView({ block: 'nearest' });
    el?.focus();
  };

  return (
    <section
      aria-label={`Sound panel for ${track.name}`}
      data-testid="sound-panel"
      className="flex flex-col gap-3 border-b border-line p-3"
    >
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={btn} disabled={!canStep} aria-label="Previous sound" data-testid="sound-prev" onClick={() => step(-1)}>
          <span className="rotate-180 inline-flex">
            <ChevronIcon size={16} />
          </span>
        </button>
        <div className="min-w-0 flex-1" aria-live="polite">
          <p className="m-0 flex items-center gap-2 truncate text-base font-semibold text-ink" data-testid="sound-panel-name">
            {category && <span aria-hidden="true" className={`size-2.5 shrink-0 rounded-full ${DOT[category]}`} />}
            <InstrumentGlyph of={null} track={track} entries={entries} size={20} data-testid="sound-panel-icon" className="shrink-0" />
            {soundLabel(track, entries)}
          </p>
          <p className="m-0 truncate text-xs text-ink-dim" data-testid="sound-panel-detail">
            {track.name}, {instrumentLabel(track)}
            {category ? `, ${category}` : ''}
          </p>
        </div>
        <button type="button" className={btn} disabled={!canStep} aria-label="Next sound" data-testid="sound-next" onClick={() => step(1)}>
          <ChevronIcon size={16} />
        </button>
        <button type="button" className={btn} onClick={replace}>
          <SearchIcon size={14} />
          Replace sound
        </button>
      </div>

      <div className="flex max-w-xl flex-col gap-2">
        <Row label="Volume" value={volumeText(mix.volume)}>
          <MixSlider
            label={`Volume of ${track.name}`}
            className={slider}
            value={mix.volume}
            min={0}
            max={config.volumeMax}
            step={config.volumeStep}
            neutral={config.volumeUnity}
            text={volumeText}
            onCommit={(v) => actions.setVolume(track.id, v)}
          />
        </Row>
        <Row label="Pan" value={panText(mix.pan)}>
          <MixSlider
            label={`Pan of ${track.name}`}
            className={slider}
            value={mix.pan}
            min={-config.panMax}
            max={config.panMax}
            step={config.panStep}
            neutral={0}
            text={panText}
            onCommit={(v) => actions.setPan(track.id, v)}
          />
        </Row>
        <Row label="Reverb send" value={trackSendText(send)}>
          <MixSlider
            label={`Reverb send of ${track.name}`}
            className={slider}
            value={send}
            min={0}
            max={config.reverbMax}
            step={config.reverbStep}
            neutral={0}
            text={trackSendText}
            onCommit={(v) => actions.setReverbSend(track.id, v)}
          />
        </Row>
        <div className="flex items-center gap-2">
          <button
            type="button"
            className={`${toggle} ${mix.mute ? 'border-accent bg-accent text-accent-ink' : 'border-line bg-panel text-ink hover:bg-panel-2'}`}
            aria-pressed={mix.mute}
            data-testid="sound-mute"
            onClick={() => void actions.setMute(track.id, !mix.mute)}
          >
            Mute
          </button>
          <button
            type="button"
            className={`${toggle} ${mix.solo ? 'border-accent bg-accent text-accent-ink' : 'border-line bg-panel text-ink hover:bg-panel-2'}`}
            aria-pressed={mix.solo}
            data-testid="sound-solo"
            onClick={() => void actions.setSolo(track.id, !mix.solo)}
          >
            Solo
          </button>
          <p className="m-0 text-xs text-ink-dim">Reverb is shared: set its room and level in the Mix tab.</p>
        </div>
      </div>
    </section>
  );
}

/** Top of the Sound tab: the selected track's sound and mix. Renders nothing without a track (the browser below explains). */
export function SoundPanel() {
  const snap = useSnapshot();
  const track = snap.project.tracks.find((t) => t.id === snap.selectedTrack);
  if (!track) return null;
  return <Panel track={track} />;
}
