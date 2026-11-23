import { useEffect, useMemo, useRef, useState, type PointerEvent } from 'react';
import type { TheoryChords } from '../backend/theory';
import { config } from '../config';
import { isTyping, chordPadIndex } from '../input/keymap';
import { liveOff, liveOn } from '../input/liveBus';
import { chordPads, type ChordPad } from '../input/screenKeys';
import { secondsToTicks } from '../time';
import { keyAt } from '../roll/keyScale';
import { btnToggle } from './classes';
import { KeyPicker } from './KeyPicker';
import { useAudioStatus, useSession, useSnapshot } from './hooks';
import { useHeld } from './useHeld';

/** Numbered chord pads for the key at the playhead. A pad plays its whole chord while held. */
export function ChordPads() {
  const { store } = useSession();
  const snap = useSnapshot();
  const status = useAudioStatus(config.meterRefreshMs);
  const [sevenths, setSevenths] = useState(false);
  const [fetched, setFetched] = useState<{ id: string; chords: TheoryChords } | null>(null);
  const tick = status.sampleRate > 0 ? secondsToTicks(snap.project.tempo_map, status.positionFrames / status.sampleRate) : 0;
  const key = keyAt(snap.project.tempo_map, tick);
  const root = key?.root;
  const scale = key?.scale;

  useEffect(() => {
    if (root === undefined || scale === undefined || !store.backend.theoryChords) return;
    let live = true;
    void store.backend.theoryChords({ root, scale }).then((r) => {
      if (live && r.ok) setFetched({ id: `${root}:${scale}`, chords: r.value });
    });
    return () => {
      live = false;
    };
  }, [store, root, scale]);

  const chords = fetched && fetched.id === `${root}:${scale}` ? fetched.chords : null;
  const pads = useMemo(() => (chords ? chordPads(sevenths ? chords.sevenths : chords.triads, config.chordPadMax, chords.root) : []), [chords, sevenths]);
  const all = useMemo(() => [...new Set(pads.flatMap((p) => p.pitches))], [pads]);
  const held = useHeld(all);
  // Pitches each pad started, so releasing one pad never cuts a note another pad shares.
  const down = useRef(new Map<number, number[]>());

  const press = (pad: ChordPad): void => {
    if (down.current.has(pad.number)) return;
    down.current.set(pad.number, pad.pitches);
    for (const p of pad.pitches) liveOn(p, config.liveVelocity);
  };
  const release = (pad: ChordPad): void => {
    const pitches = down.current.get(pad.number);
    if (!pitches) return;
    down.current.delete(pad.number);
    for (const p of pitches) liveOff(p);
  };

  // Number keys 1 to 7 play the pads while this panel is open.
  useEffect(() => {
    const find = (e: KeyboardEvent): ChordPad | undefined => {
      const i = chordPadIndex(e);
      return i === null ? undefined : pads[i];
    };
    const onDown = (e: KeyboardEvent): void => {
      if (e.repeat || isTyping(e.target)) return;
      const pad = find(e);
      if (!pad) return;
      e.preventDefault();
      press(pad);
    };
    const onUp = (e: KeyboardEvent): void => {
      const pad = find(e);
      if (pad) release(pad);
    };
    const started = down.current;
    window.addEventListener('keydown', onDown);
    window.addEventListener('keyup', onUp);
    return () => {
      window.removeEventListener('keydown', onDown);
      window.removeEventListener('keyup', onUp);
      for (const pitches of started.values()) for (const p of pitches) liveOff(p);
      started.clear();
    };
  }, [pads]);

  if (!key) {
    return (
      <div className="flex flex-wrap items-center gap-3 text-sm text-ink-dim" data-testid="chord-pads-empty">
        <span>Pick a key to get chord pads.</span>
        <KeyPicker />
      </div>
    );
  }

  return (
    <div className="flex min-w-0 flex-col gap-2" data-testid="chord-pads">
      <div className="flex items-center gap-2 text-sm text-ink-dim">
        <span>
          Chords in {chords?.rootName ?? ''} {scale?.replace('_', ' ')}
        </span>
        <button type="button" className={`${btnToggle(sevenths)} h-7`} aria-pressed={sevenths} data-testid="chord-sevenths" onClick={() => setSevenths(!sevenths)}>
          7ths
        </button>
        <span className="hidden sm:inline">Press 1 to 7 or click a pad</span>
      </div>
      <div className="flex gap-1.5" role="group" aria-label="Chord pads">
        {pads.map((pad) => {
          const lit = pad.pitches.length > 0 && pad.pitches.every((p) => held.has(p));
          return (
            <button
              key={pad.number}
              type="button"
              aria-label={`Chord ${pad.number}, ${pad.name}`}
              aria-pressed={lit}
              data-testid={`chord-pad-${pad.number}`}
              data-pitches={pad.pitches.join(',')}
              className={`flex h-12 min-w-0 flex-1 touch-none select-none flex-col items-center justify-center rounded-md border text-sm font-medium motion-safe:transition-colors ${
                lit ? 'border-accent bg-accent text-accent-ink' : 'border-line bg-panel-2 text-ink hover:bg-panel'
              }`}
              onPointerDown={(e: PointerEvent<HTMLButtonElement>) => {
                e.currentTarget.setPointerCapture(e.pointerId);
                press(pad);
              }}
              onPointerUp={() => release(pad)}
              onPointerCancel={() => release(pad)}
              onLostPointerCapture={() => release(pad)}
              onKeyDown={(e) => {
                if (e.repeat || (e.key !== 'Enter' && e.key !== ' ')) return;
                e.preventDefault();
                press(pad);
              }}
              onKeyUp={(e) => {
                if (e.key === 'Enter' || e.key === ' ') release(pad);
              }}
              onBlur={() => release(pad)}
            >
              <span className="text-xs opacity-70">{pad.number}</span>
              <span>{pad.name}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
