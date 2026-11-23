import { useCallback, useEffect, useRef, useState, useSyncExternalStore, type PointerEvent } from 'react';
import type { TheoryChords } from '../backend/theory';
import { config } from '../config';
import { liveOff, liveOn } from '../input/liveBus';
import type { Region } from '../model';
import { getPrefs, setPrefs, subscribePrefs } from '../prefs';
import { keyAt, keyName } from '../roll/keyScale';
import { chordTick, invert, type ChordMode } from '../state/chordPlan';
import { barAt, secondsToTicks } from '../time';
import { btnToggle, compact, field } from './classes';
import { ChevronDownIcon } from '../icons';
import { useAudioStatus, useSession, useSnapshot } from './hooks';

const OPEN_PREF = (): boolean => getPrefs().chordStrip;

function useStripOpen(): boolean {
  return useSyncExternalStore(subscribePrefs, OPEN_PREF, OPEN_PREF);
}

/** Header button that folds the strip away; the choice is remembered in prefs. */
export function ChordStripToggle() {
  const open = useStripOpen();
  return (
    <button
      type="button"
      className={compact(btnToggle(open))}
      aria-pressed={open}
      aria-expanded={open}
      aria-controls="chord-strip"
      data-testid="chord-strip-toggle"
      title="Show or hide the chords of the key. One click puts a chord into the clip"
      onClick={() => setPrefs({ chordStrip: !open })}
    >
      Chords
    </button>
  );
}

const compactSm = (cls: string): string => compact(cls).replace('h-8', 'h-7');

const INVERSIONS = ['Root position', 'First inversion', 'Second inversion', 'Third inversion'] as const;

/** The chords of the project key as plain names. Click puts one into the open clip, hover plays it. */
export function ChordStrip({ region, gridTicks }: { region: Region; gridTicks: number }) {
  const open = useStripOpen();
  const { store, audio, actions, record } = useSession();
  const snap = useSnapshot();
  const status = useAudioStatus(config.meterRefreshMs);
  const [mode, setMode] = useState<ChordMode>('bar');
  const [sevenths, setSevenths] = useState(false);
  const [more, setMore] = useState(false);
  const [numerals, setNumerals] = useState(false);
  const [inversion, setInversion] = useState(0);
  const [fetched, setFetched] = useState<{ id: string; chords: TheoryChords } | null>(null);
  const sounding = useRef<number[]>([]);
  const timers = useRef<{ start?: number; stop?: number }>({});

  const map = snap.project.tempo_map;
  const playhead = status.sampleRate > 0 ? secondsToTicks(map, status.positionFrames / status.sampleRate) : 0;
  const at = chordTick(region, playhead);
  const key = keyAt(map, at);
  const root = key?.root;
  const scale = key?.scale;

  useEffect(() => {
    if (!open || root === undefined || scale === undefined || !store.backend.theoryChords) return;
    let live = true;
    void store.backend.theoryChords({ root, scale }).then((r) => {
      if (live && r.ok) setFetched({ id: `${root}:${scale}`, chords: r.value });
    });
    return () => {
      live = false;
    };
  }, [open, store, root, scale]);

  const stopPreview = useCallback((): void => {
    window.clearTimeout(timers.current.start);
    window.clearTimeout(timers.current.stop);
    for (const p of sounding.current) liveOff(p);
    sounding.current = [];
  }, []);

  useEffect(() => stopPreview, [stopPreview]);

  const startPreview = useCallback(
    (pitches: number[]): void => {
      stopPreview();
      // A hover must never reach a take that is being recorded.
      if (record.getSnapshot().phase !== 'idle') return;
      timers.current.start = window.setTimeout(() => {
        sounding.current = pitches;
        for (const p of pitches) liveOn(p, config.liveVelocity);
        timers.current.stop = window.setTimeout(stopPreview, config.chordPreviewMs);
      }, config.chordPreviewDelayMs);
    },
    [record, stopPreview],
  );

  if (!open) return null;

  const chords = fetched && fetched.id === `${root}:${scale}` ? fetched.chords : null;
  const list = chords ? (sevenths ? chords.sevenths : chords.triads) : [];
  const maxInversion = Math.max(0, (list[0]?.pitches.length ?? 3) - 1);
  const steps = Math.min(inversion, maxInversion);
  const barNumber = barAt(map, at) + 1;

  // Read the playhead at click time, so a click never uses a stale poll.
  const audioTick = (): number => {
    const s = audio.status;
    return s && s.sampleRate > 0 ? secondsToTicks(store.getSnapshot().project.tempo_map, s.positionFrames / s.sampleRate) : playhead;
  };

  const put = (name: string, pitches: number[]): void => {
    stopPreview();
    const now = audioTick();
    void actions.placeChord(region.id, name, invert(pitches, steps), now, mode, gridTicks);
  };
  return (
    <div id="chord-strip" data-testid="chord-strip" className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-3 py-0.5" role="toolbar" aria-label="Chords of the key">
      {!key ? (
        <p className="m-0 text-sm text-ink-dim" data-testid="chord-strip-empty">
          Pick a key in the top bar to see its chords.
        </p>
      ) : (
        <>
          <span className="text-sm text-ink-dim">Chords in {keyName(key)}</span>
          <div className="flex items-center gap-1" role="group" aria-label="Chord buttons">
            {list.map((c) => (
              <button
                key={c.degree}
                type="button"
                data-testid={`chord-${c.degree}`}
                data-chord={c.name}
                data-pitches={invert(c.pitches, steps).join(',')}
                aria-label={`${c.name} chord (${c.roman})`}
                title={`${c.name} (${c.roman}). Click to ${mode === 'bar' ? `replace bar ${barNumber}` : `add at bar ${barNumber}`}, hover to hear it`}
                className="inline-flex h-7 min-w-10 flex-col items-center justify-center rounded-md border border-line bg-panel px-2 text-sm font-medium leading-none text-ink hover:bg-panel-2 motion-safe:transition-colors"
                onClick={() => put(c.name, c.pitches)}
                onPointerEnter={(e: PointerEvent<HTMLButtonElement>) => {
                  if (e.pointerType === 'mouse') startPreview(invert(c.pitches, steps));
                }}
                onPointerLeave={stopPreview}
                onFocus={(e) => {
                  if (e.currentTarget.matches(':focus-visible')) startPreview(invert(c.pitches, steps));
                }}
                onBlur={stopPreview}
              >
                <span>{c.name}</span>
                {numerals && <span className="text-[10px] font-normal text-ink-dim">{c.roman}</span>}
              </button>
            ))}
          </div>
          <div className="flex items-center gap-1" role="group" aria-label="Where the chord goes">
            <button type="button" className={compactSm(btnToggle(mode === 'bar'))} aria-pressed={mode === 'bar'} data-testid="chord-mode-bar" title={`Replace the notes of bar ${barNumber} with the chord`} onClick={() => setMode('bar')}>
              Replace bar {barNumber}
            </button>
            <button type="button" className={compactSm(btnToggle(mode === 'playhead'))} aria-pressed={mode === 'playhead'} data-testid="chord-mode-playhead" title="Add the chord at the playhead for one grid step" onClick={() => setMode('playhead')}>
              Add at playhead
            </button>
          </div>
          <button type="button" className={compactSm(btnToggle(sevenths))} aria-pressed={sevenths} data-testid="chord-sevenths-strip" title="Four note chords" onClick={() => setSevenths((v) => !v)}>
            7ths
          </button>
          <button type="button" className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-sm text-ink-dim hover:bg-panel-2 hover:text-ink" aria-expanded={more} aria-controls="chord-more" data-testid="chord-more" onClick={() => setMore((v) => !v)}>
            More
            <ChevronDownIcon size={14} className={more ? 'rotate-180' : ''} />
          </button>
          {more && (
            <div id="chord-more" className="flex items-center gap-3">
              <label className="flex items-center gap-1.5 text-sm text-ink-dim">
                Voicing
                <select className={compactSm(field)} value={steps} aria-label="Chord voicing (inversion)" data-testid="chord-inversion" onChange={(e) => setInversion(Number(e.target.value))}>
                  {INVERSIONS.slice(0, maxInversion + 1).map((label, i) => (
                    <option key={label} value={i}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex items-center gap-1.5 text-sm text-ink-dim">
                <input type="checkbox" className="accent-accent" checked={numerals} onChange={(e) => setNumerals(e.target.checked)} data-testid="chord-numerals" />
                Show roman numerals
              </label>
            </div>
          )}
        </>
      )}
    </div>
  );
}
