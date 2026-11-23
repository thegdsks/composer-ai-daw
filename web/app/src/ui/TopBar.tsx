import {
  ClockIcon,
  FollowIcon,
  KeyboardIcon,
  LoopIcon,
  MetronomeIcon,
  MidiIcon,
  PauseIcon,
  PlayIcon,
  RedoIcon,
  SearchIcon,
  StopIcon,
  UndoIcon,
} from '../icons';
import { bpmAt, meterSegments } from '../time';
import { config } from '../config';
import { tip } from '../input/keymap';
import { btn as btnFull, btnPrimary as btnPrimaryFull, btnToggle as btnToggleFull, compact, field as fieldFull, iconBtn as iconBtnFull } from './classes';

// One row at 1280 wide: the bar uses the compact control size and tight padding.
const tight = (cls: string): string => compact(cls).replace('px-3', 'px-2');
const btn = tight(btnFull);
const btnPrimary = tight(btnPrimaryFull);
const btnToggle = (on: boolean): string => tight(btnToggleFull(on));
const field = compact(fieldFull);
const iconBtn = compact(iconBtnFull);
import { MoreMenu } from './MoreMenu';
import { RecordControls, TidySelect } from './RecordControls';
import { SettingsMenu } from './SettingsMenu';
import { CommitText } from './CommitText';
import { KeyPicker } from './KeyPicker';
import { ProjectMenu } from './ProjectMenu';
import { PositionReadout } from './PositionReadout';
import { TapTempoButton } from './TapTempoButton';
import { useAudioStatus, useSession, useSnapshot } from './hooks';
import { useProposalCount } from './useAssist';

const METERS = ['2/4', '3/4', '4/4', '5/4', '6/8', '7/8', '9/8', '12/8'];

export function TopBar(props: {
  midi: { supported: boolean; inputs: number | null; toggle(): void };
  onHelp(): void;
  onPalette(): void;
  onHistory(): void;
  historyOpen: boolean;
}) {
  const { store, actions, audio, transport } = useSession();
  const snap = useSnapshot();
  const waiting = useProposalCount();
  const status = useAudioStatus(config.meterRefreshMs);
  const undoTip = tip(snap.undoDepth > 0 ? `Undo ${store.stepName(snap.undoDepth)}` : 'Nothing to undo', 'undo');
  const redoTip = tip(snap.redoDepth > 0 ? `Redo ${store.stepName(snap.undoDepth + 1)}` : 'Nothing to redo', 'redo');
  const playing = status.transport === 'playing';
  const prefs = snap.transport;
  const counts = Array.from({ length: config.countInMaxBars + 1 }, (_, i) => i);
  const bpm = bpmAt(snap.project.tempo_map, 0);
  const sig = meterSegments(snap.project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 };

  return (
    <header className="flex flex-wrap items-center gap-x-1.5 gap-y-1 min-h-12 border-b border-line bg-panel px-3 py-1">
      <div className="flex items-center gap-2">
        <h1 className="text-base font-semibold tracking-tight max-[2000px]:sr-only">Composer AI DAW</h1>
        <CommitText
          label="Project name"
          className={`${field} w-48 font-medium max-[2000px]:w-24`}
          value={snap.project.name}
          onCommit={(v) => void actions.rename(v)}
        />
      </div>

      <div className="flex items-center gap-2">
        <label className="flex items-center gap-1.5 text-sm text-ink-dim">
          <span className="max-[1400px]:sr-only">Tempo</span>
          <CommitText
            label="Tempo in beats per minute"
            inputMode="decimal"
            className={`${field} w-16 text-center tabular-nums text-ink`}
            value={String(Math.round(bpm * 100) / 100)}
            onCommit={(v) => void actions.setBpm(Number(v))}
          />
        </label>
        <KeyPicker />
      </div>

      <div className="flex items-center gap-1" role="group" aria-label="Transport" data-space-play>
        <button
          type="button"
          className={btnPrimary}
          aria-label={playing ? 'Pause (Space)' : 'Play (Space)'}
          title={playing ? 'Pause (Space)' : 'Play (Space)'}
          onClick={() => void audio.toggle()}
        >
          {playing ? <PauseIcon size={18} /> : <PlayIcon size={18} />}
          {playing ? 'Pause' : 'Play'}
        </button>
        <button
          type="button"
          className={btn}
          aria-label="Stop (twice to go back to where Play started)"
          title="Stop. Press again to go back to where Play started"
          onClick={() => audio.stop()}
        >
          <StopIcon size={18} />
          <span className="max-[2000px]:sr-only">Stop</span>
        </button>
        <PositionReadout />
      </div>

      <div className="flex items-center gap-1" role="group" aria-label="Loop and click" data-transport-keys>
        <button
          type="button"
          className={btnToggle(prefs.loop.on)}
          aria-pressed={prefs.loop.on}
          title={tip('Repeat the loop range', 'loop')}
          onClick={() => void transport.toggleLoop()}
        >
          <LoopIcon size={18} />
          <span className="max-[2000px]:sr-only">Loop</span>
        </button>
        <button
          type="button"
          className={btnToggle(prefs.metronome.on)}
          aria-pressed={prefs.metronome.on}
          title={tip('Click on every beat', 'metronome')}
          onClick={() => void transport.toggleMetronome()}
        >
          <MetronomeIcon size={18} />
          <span className="max-[2000px]:sr-only">Metronome</span>
        </button>
        <button
          type="button"
          className={btnToggle(prefs.follow)}
          aria-pressed={prefs.follow}
          title="Scroll along with the playhead"
          onClick={() => void transport.toggleFollow()}
        >
          <FollowIcon size={18} />
          <span className="max-[2000px]:sr-only">Follow</span>
        </button>
        {status.counting && (
          <span data-testid="counting-in" className="text-sm font-medium text-record">
            Counting in
          </span>
        )}
      </div>

      <RecordControls />

      <div className="flex items-center gap-1" role="group" aria-label="History">
        <button
          type="button"
          className={iconBtn}
          aria-label={undoTip}
          title={undoTip}
          disabled={snap.undoDepth === 0}
          onClick={() => void store.undo()}
        >
          <UndoIcon size={20} />
        </button>
        <button
          type="button"
          className={iconBtn}
          aria-label={redoTip}
          title={redoTip}
          disabled={snap.redoDepth === 0}
          onClick={() => void store.redo()}
        >
          <RedoIcon size={20} />
        </button>
      </div>

      <div className="ml-auto flex flex-wrap items-center gap-2">
        <MoreMenu waiting={waiting}>
          <TapTempoButton />
        <label className="flex items-center justify-between gap-2 text-sm text-ink-dim">
          Beats per bar
          <select
            className={field}
            aria-label="Time signature"
            value={`${sig.numerator}/${sig.denominator}`}
            onChange={(e) => {
              const [n, d] = e.target.value.split('/').map(Number);
              if (n && d) void store.apply({ command: 'set_time_signature', tick: 0, numerator: n, denominator: d });
            }}
          >
            {(METERS.includes(`${sig.numerator}/${sig.denominator}`) ? METERS : [`${sig.numerator}/${sig.denominator}`, ...METERS]).map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center justify-between gap-2 text-sm text-ink-dim">
          Count in
          <select
            className={field}
            aria-label="Count in before Play"
            title="Bars of clicks before Play starts (needs the metronome on)"
            value={prefs.countInBars}
            onChange={(e) => void transport.setCountIn(Number(e.target.value))}
          >
            {counts.map((n) => (
              <option key={n} value={n}>
                {n === 0 ? 'Off' : n === 1 ? '1 bar' : `${n} bars`}
              </option>
            ))}
          </select>
        </label>
          <TidySelect />
        {props.midi.supported && (
          <button
            type="button"
            className={`${btn} w-full`}
            aria-pressed={props.midi.inputs !== null}
            title="Play notes from a MIDI keyboard"
            onClick={props.midi.toggle}
          >
            <MidiIcon size={18} />
            {props.midi.inputs === null ? 'MIDI keyboard' : `MIDI on (${props.midi.inputs})`}
          </button>
        )}
          <button
            type="button"
            className={`${btn} w-full justify-start`}
            aria-label="History (Ctrl or Cmd Alt Z)"
            title="History (Ctrl or Cmd Alt Z)"
            aria-expanded={props.historyOpen}
            aria-controls="history-panel"
            data-testid="history-toggle"
            onClick={props.onHistory}
          >
            <ClockIcon size={18} />
            History
          </button>
          <button
            type="button"
            className={`${btn} w-full justify-start`}
            aria-label="Find a command (Ctrl or Cmd K)"
            title="Find a command (Ctrl or Cmd K)"
            data-testid="palette-open"
            onClick={props.onPalette}
          >
            <SearchIcon size={18} />
            Find a command
          </button>
        </MoreMenu>
        <ProjectMenu onHelp={props.onHelp} pluginLoaded={status.plugin !== null} />
        <button type="button" className={`${iconBtn} max-[2000px]:hidden`} aria-label="Keys and help (Shift /)" title="Keys and help (Shift /)" onClick={props.onHelp}>
          <KeyboardIcon size={20} />
        </button>
        <SettingsMenu />
      </div>
    </header>
  );
}
