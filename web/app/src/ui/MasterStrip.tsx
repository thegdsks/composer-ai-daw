import { config } from '../config';
import { volumeText } from '../state/mix';
import { masterOf, reverbText } from '../state/soundPanel';
import { useSession, useSnapshot } from './hooks';
import { MixSlider } from './MixSlider';

const slider = 'h-6 w-full min-w-0 accent-accent';

/** Small strip above the Mix faders: master volume and the shared reverb (room size and level). */
export function MasterStrip() {
  const { project } = useSnapshot();
  const { actions } = useSession();
  const m = masterOf(project);
  return (
    <div
      role="group"
      aria-label="Master"
      data-testid="master-strip"
      className="grid shrink-0 grid-cols-[repeat(auto-fit,minmax(11rem,1fr))] items-center gap-x-4 gap-y-1 border-t border-line px-3 py-1"
    >
      <div className="flex flex-col text-xs text-ink-dim">
        <span>Master volume, {volumeText(m.volume)}</span>
        <MixSlider
          label="Master volume"
          className={slider}
          value={m.volume}
          min={0}
          max={config.volumeMax}
          step={config.volumeStep}
          neutral={config.volumeUnity}
          text={volumeText}
          onCommit={(v) => actions.setMasterVolume(v)}
        />
      </div>
      <div className="flex flex-col text-xs text-ink-dim">
        <span>Reverb level, {reverbText(m.reverb_mix)}</span>
        <MixSlider
          label="Reverb level"
          className={slider}
          value={m.reverb_mix}
          min={0}
          max={config.reverbMax}
          step={config.reverbStep}
          neutral={0}
          text={reverbText}
          onCommit={(v) => actions.setMasterReverb(m.reverb_size, v)}
        />
      </div>
      <div className="flex flex-col text-xs text-ink-dim">
        <span>Reverb room size, {reverbText(m.reverb_size)}</span>
        <MixSlider
          label="Reverb room size"
          className={slider}
          value={m.reverb_size}
          min={0}
          max={config.reverbMax}
          step={config.reverbStep}
          neutral={config.reverbSizeDefault}
          text={reverbText}
          onCommit={(v) => actions.setMasterReverb(v, m.reverb_mix)}
        />
      </div>
    </div>
  );
}
