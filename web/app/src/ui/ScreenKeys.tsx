import { useMemo, useRef, useSyncExternalStore, type KeyboardEvent, type PointerEvent } from 'react';
import { config } from '../config';
import { liveOff, liveOn } from '../input/liveBus';
import { getLiveOctave, setLiveOctave, subscribeLiveOctave } from '../input/liveOctave';
import { keyGroups } from '../input/screenKeys';
import { pitchName } from '../time';
import { iconBtn } from './classes';
import { MinusIcon, PlusIcon } from '../icons';
import { useHeld } from './useHeld';

/** Two octaves you can play with the pointer or the keyboard; keys also light for computer and MIDI notes. */
export function ScreenKeys() {
  const octave = useSyncExternalStore(subscribeLiveOctave, getLiveOctave, getLiveOctave);
  const groups = useMemo(() => keyGroups(octave, config.screenKeyOctaves), [octave]);
  const pitches = useMemo(() => groups.flatMap((g) => (g.black === null ? [g.white] : [g.white, g.black])), [groups]);
  const held = useHeld(pitches);
  // Pitches this component started, so a key up never silences a note somebody else is holding.
  const mine = useRef(new Set<number>());

  const on = (pitch: number): void => {
    if (mine.current.has(pitch)) return;
    mine.current.add(pitch);
    liveOn(pitch, config.liveVelocity);
  };
  const off = (pitch: number): void => {
    if (!mine.current.delete(pitch)) return;
    liveOff(pitch);
  };
  const handlers = (pitch: number) => ({
    onPointerDown: (e: PointerEvent<HTMLButtonElement>) => {
      e.currentTarget.setPointerCapture(e.pointerId);
      on(pitch);
    },
    onPointerUp: () => off(pitch),
    onPointerCancel: () => off(pitch),
    onLostPointerCapture: () => off(pitch),
    onKeyDown: (e: KeyboardEvent<HTMLButtonElement>) => {
      if (e.repeat || (e.key !== 'Enter' && e.key !== ' ')) return;
      e.preventDefault();
      on(pitch);
    },
    onKeyUp: (e: KeyboardEvent<HTMLButtonElement>) => {
      if (e.key === 'Enter' || e.key === ' ') off(pitch);
    },
    onBlur: () => off(pitch),
  });
  const lowName = pitchName(config.keyboardBaseNote + octave * 12);

  return (
    <div className="flex min-w-0 flex-col gap-2" data-testid="screen-keys">
      <div className="flex items-center gap-1 text-sm text-ink-dim" role="group" aria-label="Octave">
        <button type="button" className={iconBtn} aria-label="Octave down" data-testid="octave-down" onClick={() => setLiveOctave(octave - 1)}>
          <MinusIcon size={16} />
        </button>
        <span className="min-w-20 text-center tabular-nums text-ink" data-testid="octave-label" aria-live="polite">
          From {lowName}
        </span>
        <button type="button" className={iconBtn} aria-label="Octave up" data-testid="octave-up" onClick={() => setLiveOctave(octave + 1)}>
          <PlusIcon size={16} />
        </button>
        <span className="ml-2 hidden sm:inline">Computer keys: A to semicolon play, Z and X move the octave</span>
      </div>
      <div className="flex h-24 touch-none select-none" role="group" aria-label="Keyboard">
        {groups.map((g) => (
          <div key={g.white} className="relative flex-1">
            <button
              type="button"
              aria-label={pitchName(g.white)}
              aria-pressed={held.has(g.white)}
              data-pitch={g.white}
              className={`flex size-full items-end justify-center rounded-b-md border border-line pb-1 text-xs motion-safe:transition-colors ${
                held.has(g.white) ? 'bg-accent text-accent-ink' : 'bg-key-white text-key-label hover:bg-panel-2'
              }`}
              {...handlers(g.white)}
            >
              {g.white % 12 === 0 ? pitchName(g.white) : ''}
            </button>
            {g.black !== null && (
              <button
                type="button"
                aria-label={pitchName(g.black)}
                aria-pressed={held.has(g.black)}
                data-pitch={g.black}
                className={`absolute -right-2 top-0 z-10 h-14 w-4 rounded-b-md border border-line motion-safe:transition-colors ${
                  held.has(g.black) ? 'bg-accent' : 'bg-key-black'
                }`}
                {...handlers(g.black)}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
