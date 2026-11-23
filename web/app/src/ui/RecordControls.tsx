import { useSyncExternalStore } from 'react';
import { gridChoices } from '../config';
import { tip } from '../input/keymap';
import { KeyboardIcon, RecordIcon } from '../icons';
import { btnToggle, field } from './classes';
import { CatchButton } from './CatchButton';
import { useSession } from './hooks';

const PHASE_TEXT = { idle: 'Record', starting: 'Getting ready', counting: 'Counting in', recording: 'Stop recording' } as const;

/** Record, the Keys setting and the timing tidy option. Everything goes through RecordActions. */
export function RecordControls() {
  const { record } = useSession();
  const state = useSyncExternalStore(record.subscribe, record.getSnapshot, record.getSnapshot);
  const armed = state.phase !== 'idle';
  return (
    <div className="flex items-center gap-1" role="group" aria-label="Record" data-transport-keys>
      <button
        type="button"
        className={btnToggle(armed)}
        data-testid="record"
        data-phase={state.phase}
        title={tip(armed ? 'Stop recording' : 'Record into the selected clip after a count in', 'record')}
        onClick={() => void record.toggle()}
      >
        <RecordIcon size={18} className={armed ? 'text-accent-ink' : 'text-record'} />
        {PHASE_TEXT[state.phase]}
      </button>
      <button
        type="button"
        className={btnToggle(state.pianoKeys)}
        aria-label="Keys: play notes with the computer keyboard"
        aria-pressed={state.pianoKeys}
        data-testid="keys-toggle"
        title={tip('Play notes with the computer keyboard (A to semicolon)', 'keys')}
        onClick={() => record.togglePianoKeys()}
      >
        <KeyboardIcon size={18} />
        <span className="hidden min-[2000px]:inline">Keys</span>
      </button>
      <CatchButton />
    </div>
  );
}

/** Timing of recorded notes. It sits in the More menu, the Record button keeps the take itself. */
export function TidySelect() {
  const { record } = useSession();
  const state = useSyncExternalStore(record.subscribe, record.getSnapshot, record.getSnapshot);
  return (
    <label className="flex items-center justify-between gap-2 text-sm text-ink-dim">
      Recorded timing
      <select
        className={`${field} w-40`}
        aria-label="Timing of recorded notes"
        title="Move recorded notes onto the nearest grid line, or keep them as played"
        data-testid="tidy"
        value={state.tidy ? state.gridId : 'off'}
        onChange={(e) => {
          if (e.target.value === 'off') record.setTidy(false);
          else {
            record.setGrid(e.target.value);
            if (!state.tidy) record.setTidy(true);
          }
        }}
      >
        <option value="off">As played</option>
        {gridChoices().map((c) => (
          <option key={c.id} value={c.id}>
            Tidy to {c.label.toLowerCase()}
          </option>
        ))}
      </select>
    </label>
  );
}
