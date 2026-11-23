import { useSyncExternalStore } from 'react';
import { tip } from '../input/keymap';
import { ClockIcon } from '../icons';
import { btn } from './classes';
import { useSession } from './hooks';

/** Dim while nothing was played lately, bright after. One click turns the notes into a clip. */
export function CatchButton() {
  const { catcher } = useSession();
  const { count } = useSyncExternalStore(catcher.subscribe, catcher.getSnapshot, catcher.getSnapshot);
  return (
    <button
      type="button"
      className={`${btn} ${count === 0 ? 'text-ink-dim' : 'border-accent'}`}
      data-testid="catch"
      data-count={count}
      aria-label={count === 0 ? 'Catch what I just played, nothing yet' : `Catch what I just played, ${count} notes`}
      title={tip('Keep what you just played as a new clip', 'catch')}
      onClick={() => void catcher.commit()}
    >
      <ClockIcon size={18} />
      <span className="hidden min-[2000px]:inline">Catch</span>
      {count > 0 && <span className="rounded-sm bg-panel-2 px-1 text-xs tabular-nums">{count}</span>}
    </button>
  );
}
