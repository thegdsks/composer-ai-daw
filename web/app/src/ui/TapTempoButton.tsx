import { useEffect, useRef, useState } from 'react';
import { config } from '../config';
import { TapIcon } from '../icons/timeline';
import { TapTempo } from '../state/tapTempo';
import { btn } from './classes';
import { useSession } from './hooks';

/**
 * Tap the beat: the average of the last taps shows on the button and, once taps stop for a
 * moment, becomes one tempo change (one undo step, not one per tap).
 */
export function TapTempoButton() {
  const { actions } = useSession();
  const tapper = useRef(new TapTempo({ resetMs: config.tapResetMs, window: config.tapWindow, minTaps: config.tapMinTaps }));
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [shown, setShown] = useState<number | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const tap = (): void => {
    const bpm = tapper.current.tap(performance.now());
    if (timer.current) clearTimeout(timer.current);
    if (bpm === null) {
      setShown(null);
      return;
    }
    const rounded = Math.round(bpm * 10) / 10;
    setShown(rounded);
    timer.current = setTimeout(() => {
      void actions.setBpm(rounded);
      setShown(null);
      tapper.current.reset();
    }, config.tapCommitMs);
  };

  return (
    <button
      type="button"
      className={btn}
      data-testid="tap-tempo"
      title="Tap the beat to set the tempo"
      aria-label={shown === null ? 'Tap tempo' : `Tap tempo, ${shown} beats per minute`}
      onClick={tap}
    >
      <TapIcon size={18} />
      <span className="min-w-8 text-center tabular-nums">{shown === null ? 'Tap' : shown}</span>
    </button>
  );
}
