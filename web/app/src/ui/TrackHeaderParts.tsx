// Small parts of the track header (U35): rename in place, color chip, arm placeholder, menu button.
// They only call actions; the reorder grip lives in useTrackReorder.
import { InstrumentGlyph } from '../icons/instruments';
import { useEffect, useRef, useState, useSyncExternalStore, type HTMLAttributes, type KeyboardEvent } from 'react';
import { GripIcon, MoreIcon, RecordIcon } from '../icons';
import type { SoundEntry } from '../backend/types';
import type { Track } from '../model';
import {
  FAMILY_CLASS,
  familyOfTrack,
  getOverrides,
  nextOverride,
  setOverride,
  subscribeOverrides,
} from '../state/trackView';
import { compact, field } from './classes';

const small =
  'inline-flex size-6 shrink-0 items-center justify-center rounded text-ink-dim hover:bg-panel-2 hover:text-ink ' +
  'disabled:cursor-not-allowed disabled:opacity-40 motion-safe:transition-colors';

/** The track name: plain text that becomes a text box on double click, F2 or Enter. */
export function TrackName({ track, onCommit }: { track: Track; onCommit(name: string): void }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(track.name);
  const input = useRef<HTMLInputElement>(null);
  const label = useRef<HTMLButtonElement>(null);
  const cancelled = useRef(false);
  const done = useRef(true);

  useEffect(() => {
    if (editing) input.current?.select();
    else if (!done.current) {
      done.current = true;
      label.current?.focus({ preventScroll: true });
    }
  }, [editing]);

  const begin = (): void => {
    cancelled.current = false;
    done.current = false;
    setDraft(track.name);
    setEditing(true);
  };
  const finish = (): void => {
    if (!editing) return;
    setEditing(false);
    if (!cancelled.current && draft.trim() && draft.trim() !== track.name) onCommit(draft);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>): void => {
    e.stopPropagation();
    if (e.key === 'Enter') finish();
    if (e.key === 'Escape') {
      cancelled.current = true;
      finish();
    }
  };

  if (editing) {
    return (
      <input
        ref={input}
        data-testid="track-name-input"
        data-autofocus=""
        autoFocus
        aria-label={`Name of ${track.name}`}
        className={`${compact(field)} min-w-0 flex-1`}
        value={draft}
        autoComplete="off"
        spellCheck={false}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={onKey}
        onBlur={finish}
      />
    );
  }
  return (
    <button
      ref={label}
      type="button"
      data-testid="track-name"
      title="Double click to rename"
      aria-label={`${track.name}. Press Enter or F2 to rename`}
      className="h-8 min-w-14 flex-1 truncate rounded-md px-2 text-left text-sm font-medium text-ink hover:bg-panel-2 motion-safe:transition-colors"
      onDoubleClick={begin}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === 'F2') {
          e.preventDefault();
          begin();
        }
      }}
    >
      {track.name}
    </button>
  );
}

/** A color chip: the family of the sound, or the pick made here. Each press steps on, the last step goes back to automatic. */
export function ColorChip({ track, entries }: { track: Track; entries: readonly SoundEntry[] }) {
  const overrides = useSyncExternalStore(subscribeOverrides, getOverrides);
  const picked = overrides[track.id];
  const family = familyOfTrack(track, entries, picked);
  return (
    <button
      type="button"
      data-testid="track-color"
      data-family={family}
      className="inline-flex size-6 shrink-0 items-center justify-center rounded hover:bg-panel-2"
      aria-label={`Color of ${track.name}: ${family}${picked ? '' : ' (from the sound)'}. Press to change`}
      title="Change the color"
      onClick={() => setOverride(track.id, nextOverride(picked))}
    >
      <span className={`size-3 rounded-sm ${FAMILY_CLASS[family]}`} />
    </button>
  );
}

/** What the track plays, as a small icon beside the color chip. Decorative, the sound is named elsewhere. */
export function TrackSoundIcon({ track, entries }: { track: Track; entries: readonly SoundEntry[] }) {
  return <InstrumentGlyph of={null} track={track} entries={entries} size={16} data-testid="track-icon" className="shrink-0 text-ink-dim" />;
}

/** Record arm has no command yet (U14 records into the selected clip), so it says so instead of hiding. */
export function ArmButton({ track }: { track: Track }) {
  const reason = 'Per track arming is not built yet. Recording goes into the selected clip';
  return (
    <button
      type="button"
      data-testid="track-arm"
      disabled
      className={small}
      aria-label={`Arm ${track.name} for recording. Unavailable: ${reason}`}
      title={reason}
    >
      <RecordIcon size={12} />
    </button>
  );
}

export function MenuButton({ track, onOpen }: { track: Track; onOpen(button: HTMLElement): void }) {
  return (
    <button
      type="button"
      data-testid="track-menu"
      className={small}
      aria-haspopup="menu"
      aria-label={`Menu for ${track.name}`}
      title="Track menu"
      onClick={(e) => onOpen(e.currentTarget)}
    >
      <MoreIcon size={14} />
    </button>
  );
}

/** Drag handle for reorder. Arrow keys move the track one row, Home and End to the ends. */
export function Grip({
  track,
  index,
  count,
  pointerProps,
  onKey,
}: {
  track: Track;
  index: number;
  count: number;
  pointerProps: HTMLAttributes<HTMLButtonElement>;
  onKey(e: KeyboardEvent<HTMLButtonElement>): void;
}) {
  return (
    <button
      type="button"
      data-grip=""
      data-testid="track-grip"
      className={`${small} cursor-grab touch-none active:cursor-grabbing`}
      aria-label={`Reorder ${track.name}, position ${index + 1} of ${count}. Arrow up and down move it`}
      title="Drag to reorder, or use the arrow keys"
      onKeyDown={onKey}
      {...pointerProps}
    >
      <GripIcon size={14} />
    </button>
  );
}
