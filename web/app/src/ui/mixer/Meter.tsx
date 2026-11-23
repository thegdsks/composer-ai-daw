import { useEffect, useState } from 'react';
import { config } from '../../config';
import { litSegments, trackMeterLevel } from '../../state/meter';
import { secondsToTicks } from '../../time';
import { useSession } from '../hooks';

const segments = Array.from({ length: config.meterSegments }, (_, i) => i);

/** Segmented level bar for one track. Only mounted strips poll, at the calm meter rate. */
export function Meter(props: { trackId: string; name: string; index: number }) {
  const { store, audio } = useSession();
  const [lit, setLit] = useState(0);
  useEffect(() => {
    const read = (): void => {
      const { project } = store.getSnapshot();
      const track = project.tracks.find((t) => t.id === props.trackId);
      const s = audio.status;
      const measured = s.trackPeaks.length > 0 && s.trackPeaks.length === project.tracks.length;
      if (!track || s.sampleRate === 0 || (s.transport !== 'playing' && !measured)) {
        setLit(0);
        return;
      }
      const tick = secondsToTicks(project.tempo_map, s.positionFrames / s.sampleRate);
      setLit(litSegments(trackMeterLevel(project, track, tick, s.transport === 'playing', s.trackPeaks)));
    };
    read();
    const timer = setInterval(read, config.meterRefreshMs);
    return () => clearInterval(timer);
  }, [store, audio, props.trackId]);

  return (
    <div
      role="meter"
      aria-label={`Level of ${props.name}`}
      aria-valuemin={0}
      aria-valuemax={config.meterSegments}
      aria-valuenow={lit}
      aria-valuetext={lit === 0 ? 'Silent' : `${Math.round((lit / config.meterSegments) * 100)} percent`}
      data-testid={`meter-${props.index}`}
      data-level={lit}
      className="flex w-2 shrink-0 flex-col-reverse gap-px self-stretch"
    >
      {segments.map((i) => (
        <span
          key={i}
          className={`min-h-0 flex-1 rounded-[1px] ${i < lit ? (i >= config.meterSegments - 2 ? 'bg-danger' : 'bg-accent') : 'bg-line'}`}
        />
      ))}
    </div>
  );
}
