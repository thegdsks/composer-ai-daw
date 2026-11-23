import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { holdNote, releaseNote } from '../input/heldNotes';
import { setLiveSink } from '../input/liveBus';
import { attachComputerKeyboard, connectMidi, midiSupported, type MidiConnection } from '../input/liveInput';
import { matchShortcut, isTextField, isTyping, spaceMeansPlay, transportKeysApply } from '../input/keymap';
import { config } from '../config';
import { secondsToTicks } from '../time';
import { getSaveStatus, subscribeSaveStatus } from '../persistence';
import { getPrefs } from '../prefs';
import { guardLeave, UnsavedGuard } from '../state/unsavedGuard';
import type { Session } from '../session';
import { Announcer } from './Announcer';
import { ArrangementPanel } from './ArrangementPanel';
import { ConfirmHost } from './Confirm';
import { Dock, type DockTab } from './Dock';
import { RepeatDialog } from './RepeatDialog';
import { detectEnvIssue, envIssue, EnvDialog, midiIssue, type EnvIssue } from './EnvDialog';
import { CommandPalette } from './CommandPalette';
import { Help } from './Help';
import { History } from './History';
import { Library } from './Library';
import { toggleLibrary } from './libraryState';
import { NextAction } from './NextAction';
import { FirstRun, Start, useStart } from './Start';
import { SessionContext } from './hooks';
import { GuardContext, ToastsContext } from './shell';
import { Toasts } from './Toasts';
import { ToastQueue } from './toastQueue';
import { PianoRoll, type RollApi } from './PianoRoll';
import { Splitter } from './Splitter';
import { StatusBar } from './StatusBar';
import { TopBar } from './TopBar';

const MESSAGE_MS = 4000;

export function App({ session }: { session: Session }) {
  const { store, audio, actions, transport, record, catcher } = session;
  const rollApi = useRef<RollApi | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [octave, setOctave] = useState(0);
  const [tab, setTab] = useState<DockTab>('notes');
  // Under 640 px only one of the two areas shows; this picks which.
  const [narrow, setNarrow] = useState<'arrangement' | 'dock'>('dock');
  const area = useRef<HTMLDivElement>(null);
  const [midiInputs, setMidiInputs] = useState<number | null>(null);
  const midiRef = useRef<MidiConnection | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const historyOpener = useRef<HTMLElement | null>(null);
  const start = useStart(session);
  const [env, setEnv] = useState<EnvIssue | null>(() => detectEnvIssue());
  const toasts = useMemo(() => new ToastQueue(() => getPrefs().toasts), []);
  const guard = useMemo(() => new UnsavedGuard(), []);
  const save = useSyncExternalStore(subscribeSaveStatus, getSaveStatus, getSaveStatus);
  const warnedStorage = useRef(false);

  // A refused autosave is shown once as a plain dialog; the footer keeps saying so after that.
  useEffect(() => {
    if (!save.failed || warnedStorage.current) return;
    warnedStorage.current = true;
    setEnv(envIssue('storage-full'));
  }, [save.failed]);

  // Deleting a track is easy to do by accident, so it gets a toast with Undo.
  useEffect(() => {
    const off = store.onAnnounce((text) => {
      if (/^delete track/i.test(text)) toasts.push({ text: 'Track deleted', action: { label: 'Undo', run: () => store.undo() } });
    });
    return () => {
      off();
      toasts.dispose();
    };
  }, [store, toasts]);

  // The first loaded project is the saved starting point; leaving with changes asks first.
  useEffect(() => {
    const check = (): void => guard.baseline(store.getSnapshot().stateHash);
    check();
    const off = store.subscribe(check);
    const onLeave = (e: BeforeUnloadEvent): void => {
      guardLeave(guard, store.getSnapshot().stateHash, e);
    };
    window.addEventListener('beforeunload', onLeave);
    return () => {
      off();
      window.removeEventListener('beforeunload', onLeave);
    };
  }, [store, guard]);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    session.setNotifier((m) => {
      setMessage(m);
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => setMessage(null), MESSAGE_MS);
    });
    return () => {
      if (timer) clearTimeout(timer);
    };
  }, [session]);

  // Live notes go to the engine track that matches the selected track.
  const liveTrack = useCallback((): number => {
    const snap = store.getSnapshot();
    return Math.max(0, snap.project.tracks.findIndex((t) => t.id === snap.selectedTrack));
  }, [store]);

  // Every live note sounds, lights its key on the roll and, while recording, joins the take.
  const sink = useRef({
    noteOn: (pitch: number, velocity: number): void => {
      audio.liveNote(liveTrack(), pitch, velocity, true);
      holdNote(pitch);
      record.noteOn(pitch, velocity);
      catcher.noteOn(pitch, velocity);
    },
    noteOff: (pitch: number): void => {
      audio.liveNote(liveTrack(), pitch, 0, false);
      releaseNote(pitch);
      record.noteOff(pitch);
      catcher.noteOff(pitch);
    },
  });

  useEffect(() => setLiveSink(sink.current), []);

  useEffect(() => attachComputerKeyboard(sink.current, setOctave, () => record.keysActive), [record]);

  const toggleHistory = useCallback((): void => {
    setHistoryOpen((open) => {
      if (!open) historyOpener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      return !open;
    });
  }, []);

  // Clipboard entries from the palette: notes when notes are selected, otherwise the selected clips.
  const runEdit = useCallback(
    (what: 'copy' | 'cut' | 'paste' | 'duplicate' | 'repeat'): void => {
      const snap = store.getSnapshot();
      const region = snap.selectedRegion;
      const notes = region !== null && snap.selectedNotes.size > 0;
      const clips = [...snap.selectedRegions];
      const grid = (config.ticksPerQuarter * 4) / config.defaultGridDenominator;
      const st = audio.status;
      const at = st.sampleRate > 0 ? secondsToTicks(snap.project.tempo_map, st.positionFrames / st.sampleRate) : 0;
      if (what === 'paste') void actions.pasteInArrangement(at);
      else if (what === 'copy') void (notes && region ? actions.copyNotes(region, snap.selectedNotes) : actions.copyClipsToClipboard(clips));
      else if (what === 'cut') void (notes && region ? actions.cutNotes(region, snap.selectedNotes) : actions.cutClips(clips));
      else if (what === 'duplicate') void (notes && region ? actions.duplicateNotes(region, snap.selectedNotes, grid) : actions.duplicateClips(clips));
      else void (notes && region ? actions.repeatNotes(region, snap.selectedNotes, grid) : actions.repeatClipsAsk(clips));
    },
    [store, audio, actions],
  );

  // Closing the panel hands the focus back to where it came from.
  const closeHistory = useCallback((): void => {
    setHistoryOpen(false);
    requestAnimationFrame(() => historyOpener.current?.focus());
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      const action = matchShortcut(e);
      if (action === 'undo' || action === 'redo') {
        if (isTextField(e.target)) return;
        e.preventDefault();
        void (action === 'redo' ? store.redo() : store.undo());
      } else if (action === 'play') {
        // Space on any other control (such as Save to file) keeps its normal job.
        if (!spaceMeansPlay(e.target)) return;
        e.preventDefault();
        void audio.toggle();
      } else if (action === 'loop' || action === 'metronome' || action === 'seekBack' || action === 'seekForward') {
        if (e.repeat || !transportKeysApply(e.target)) return;
        e.preventDefault();
        if (action === 'loop') void transport.toggleLoop();
        else if (action === 'metronome') void transport.toggleMetronome();
        else void transport.seekBars(action === 'seekBack' ? -1 : 1);
      } else if (action === 'record') {
        if (e.repeat || isTyping(e.target)) return;
        e.preventDefault();
        void record.toggle();
      } else if (action === 'keys') {
        // Plain K is a piano key while Keys is on, so only Shift K turns it off.
        if (e.repeat || isTyping(e.target)) return;
        if (record.keysActive && !e.shiftKey) return;
        e.preventDefault();
        record.togglePianoKeys();
      } else if (action === 'catch') {
        if (e.repeat || isTyping(e.target)) return;
        e.preventDefault();
        void catcher.commit();
      } else if (action === 'delete') {
        if (isTyping(e.target)) return;
        e.preventDefault();
        void rollApi.current?.deleteSelection();
      } else if (action === 'palette') {
        // Another dialog (help, a file name) owns the keyboard while it is open.
        if (e.target instanceof HTMLElement && e.target.closest('dialog') !== null) return;
        e.preventDefault();
        setPaletteOpen(true);
      } else if (action === 'history') {
        e.preventDefault();
        toggleHistory();
      } else if (action === 'library') {
        if (e.repeat || isTyping(e.target)) return;
        e.preventDefault();
        toggleLibrary();
      } else if (action === 'help') {
        if (isTyping(e.target)) return;
        e.preventDefault();
        setHelpOpen(true);
      }
    };
    // The key up of a handled Space must not click a focused transport button a second time.
    const onKeyUp = (e: KeyboardEvent): void => {
      if (e.code === 'Space' && spaceMeansPlay(e.target)) e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('keyup', onKeyUp);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('keyup', onKeyUp);
    };
  }, [store, audio, transport, record, catcher, toggleHistory]);

  // Enter on a clip: show the Notes editor (and on a narrow screen its area) and focus the roll.
  const openClip = useCallback(
    (regionId: string): void => {
      const track = store.getSnapshot().project.tracks.find((t) => t.regions[regionId] !== undefined);
      setTab('notes');
      setNarrow('dock');
      if (track) store.say(`Editing ${track.name} in Notes`);
      requestAnimationFrame(() => document.getElementById('piano-roll')?.focus());
    },
    [store],
  );

  const onApi = useCallback((api: RollApi | null) => {
    rollApi.current = api;
  }, []);
  const preview = useCallback((pitch: number, on: boolean) => {
    if (on) sink.current.noteOn(pitch, config.liveVelocity);
    else sink.current.noteOff(pitch);
  }, []);

  const toggleMidi = useCallback(() => {
    if (midiRef.current) {
      midiRef.current.close();
      midiRef.current = null;
      setMidiInputs(null);
      return;
    }
    connectMidi(sink.current)
      .then((c) => {
        midiRef.current = c;
        setMidiInputs(c.inputs);
      })
      .catch((e: unknown) => setEnv(midiIssue(e)));
  }, []);

  useEffect(() => () => midiRef.current?.close(), []);

  return (
    <SessionContext.Provider value={session}>
      <ToastsContext.Provider value={toasts}>
        <GuardContext.Provider value={guard}>
          <ConfirmHost>
            <div className="flex h-full select-none flex-col">
              <a
                href="#piano-roll"
                className="sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-10 focus:rounded-md focus:bg-accent focus:px-3 focus:py-2 focus:text-accent-ink"
                onClick={(e) => {
                  e.preventDefault();
                  document.getElementById('piano-roll')?.focus();
                }}
              >
                Skip to the piano roll
              </a>
              <a
                href="#arrangement-canvas"
                className="sr-only focus:not-sr-only focus:absolute focus:left-48 focus:top-2 focus:z-10 focus:rounded-md focus:bg-accent focus:px-3 focus:py-2 focus:text-accent-ink"
                onClick={(e) => {
                  e.preventDefault();
                  document.getElementById('arrangement-canvas')?.focus();
                }}
              >
                Skip to the arrangement
              </a>
              <TopBar midi={{ supported: midiSupported(), inputs: midiInputs, toggle: toggleMidi }} onHelp={() => setHelpOpen(true)} onPalette={() => setPaletteOpen(true)} onHistory={toggleHistory} historyOpen={historyOpen} />
              <NextAction hidden={start.open} onStart={start.show} onTab={setTab} onAssist={() => setPaletteOpen(true)} />
              <div className="flex min-h-0 flex-1">
                <Library />
                <main
                  ref={area}
                  aria-label="Workspace"
                  className="relative grid min-h-0 min-w-0 flex-1 grid-rows-[auto_minmax(0,1fr)] sm:grid-rows-[clamp(var(--arr-min),var(--split),calc(100%-var(--dock-min)))_auto_minmax(0,1fr)]"
                >
                  <Start state={start} />
                  <div role="group" aria-label="Show" className="flex border-b border-line bg-panel sm:hidden">
                    {(['arrangement', 'dock'] as const).map((v) => (
                      <button
                        key={v}
                        type="button"
                        aria-pressed={narrow === v}
                        className={`h-9 flex-1 text-sm font-medium ${narrow === v ? 'border-b-2 border-accent text-ink' : 'text-ink-dim'}`}
                        onClick={() => setNarrow(v)}
                      >
                        {v === 'arrangement' ? 'Tracks' : 'Editor'}
                      </button>
                    ))}
                  </div>
                  <div className={`flex min-h-0 flex-col ${narrow === 'dock' ? 'max-sm:hidden' : ''}`}>
                    <ArrangementPanel onOpenClip={openClip} />
                  </div>
                  <Splitter area={area} />
                  <div className={`flex min-h-0 flex-col ${narrow === 'arrangement' ? 'max-sm:hidden' : ''}`}>
                    <Dock tab={tab} onTab={setTab} notes={<PianoRoll onApi={onApi} preview={preview} />} />
                  </div>
                  <FirstRun hidden={start.open} />
                </main>
              </div>
              <Toasts />
              <History open={historyOpen} onClose={closeHistory} />
              <CommandPalette
                open={paletteOpen}
                onClose={() => setPaletteOpen(false)}
                app={{
                  play: () => void audio.toggle(),
                  undo: () => void store.undo(),
                  redo: () => void store.redo(),
                  history: toggleHistory,
                  help: () => setHelpOpen(true),
                  start: start.show,
                  edit: runEdit,
                  quantize: () => void rollApi.current?.quantize(),
                  transport: (what) => void (what === 'loop' ? transport.toggleLoop() : transport.toggleMetronome()),
                }}
              />
              <Help open={helpOpen} onClose={() => setHelpOpen(false)} onStart={start.show} />
              <EnvDialog issue={env} onClose={() => setEnv(null)} />
              <RepeatDialog />
              <Announcer />
              <StatusBar message={message} octave={octave} />
            </div>
          </ConfirmHost>
        </GuardContext.Provider>
      </ToastsContext.Provider>
    </SessionContext.Provider>
  );
}
