import { config } from '../config';
import { secondsToTicks } from '../time';
import { formatPosition } from '../state/position';
import { useAudioStatus, useSnapshot } from './hooks';

/** Playhead position as bar.beat.sixteenth and as minutes and seconds, beside the transport. */
export function PositionReadout() {
  const status = useAudioStatus(config.positionRefreshMs);
  const map = useSnapshot().project.tempo_map;
  const tick = status.sampleRate > 0 ? secondsToTicks(map, status.positionFrames / status.sampleRate) : 0;
  const p = formatPosition(map, tick);
  return (
    <div
      role="group"
      aria-label="Playhead position"
      data-testid="position-readout"
      className="flex items-baseline gap-2 rounded-md border border-line bg-panel-2 px-2 py-1 text-sm tabular-nums text-ink"
    >
      <span title="Bar, beat and sixteenth" data-testid="position-musical" className="min-w-14 text-right font-semibold">
        {p.musical}
      </span>
      <span title="Minutes and seconds" data-testid="position-time" className="min-w-16 text-right text-ink-dim">
        {p.time}
      </span>
    </div>
  );
}
