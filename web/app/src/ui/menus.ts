// Item lists for each context menu. Every item calls an existing action or command; the UI adds no edit of its own.
import type { SoundEntry } from '../backend/types';
import type { Id, Track } from '../model';
import type { MenuItem } from './menuModel';

export interface TrackMenuDeps {
  rename(track: Track): void;
  move(track: Id, delta: -1 | 1): void;
  duplicate(track: Track): void;
  remove(track: Track): void;
}

export function trackMenuItems(track: Track, index: number, count: number, deps: TrackMenuDeps): MenuItem[] {
  return [
    { id: 'rename', label: 'Rename track', run: () => deps.rename(track) },
    { id: 'move-up', label: 'Move up', disabled: index <= 0, reason: 'Already the first track', run: () => deps.move(track.id, -1) },
    { id: 'move-down', label: 'Move down', disabled: index >= count - 1, reason: 'Already the last track', run: () => deps.move(track.id, 1) },
    { id: 'duplicate', label: 'Duplicate track', run: () => deps.duplicate(track) },
    { id: 'delete', label: 'Delete track', separated: true, danger: true, run: () => deps.remove(track) },
  ];
}

export interface ClipMenuDeps {
  open(): void;
  split(): void;
  duplicate(): void;
  remove(): void;
  /** Loop length sheet; `loopText` is the current loop, null for a clip that cannot loop (audio). */
  loop?(): void;
  loopText?: string | null;
}

export function clipMenuItems(selected: number, deps: ClipMenuDeps): MenuItem[] {
  const many = selected > 1;
  const loop: MenuItem[] = deps.loop
    ? [
        {
          id: 'loop',
          label: deps.loopText ? `Loop length: ${deps.loopText}` : 'Loop length',
          disabled: many || deps.loopText == null,
          reason: many ? 'Pick one clip' : 'Only note clips can loop',
          run: deps.loop,
        },
      ]
    : [];
  return [
    { id: 'open', label: 'Open in Notes', shortcut: 'Enter', disabled: many, reason: 'Pick one clip', run: deps.open },
    ...loop,
    { id: 'split', label: 'Split at the playhead', shortcut: 'S', run: deps.split },
    { id: 'duplicate', label: many ? `Duplicate ${selected} clips` : 'Duplicate clip', shortcut: 'Ctrl D', run: deps.duplicate },
    { id: 'delete', label: many ? `Delete ${selected} clips` : 'Delete clip', shortcut: 'Delete', separated: true, danger: true, run: deps.remove },
  ];
}

/** The keyboard twin of a drop: pick the sound row, open its menu, choose where it goes. */
export function soundMenuItems(entry: SoundEntry, tracks: readonly Track[], pick: (target: { kind: 'track'; trackId: Id } | { kind: 'new-track' }) => void): MenuItem[] {
  const items: MenuItem[] = tracks.map((t) => ({
    id: `use-${t.id}`,
    label: `Use on ${t.name}`,
    run: () => pick({ kind: 'track', trackId: t.id }),
  }));
  items.push({ id: 'new-track', label: `Add a track with ${entry.name}`, separated: tracks.length > 0, run: () => pick({ kind: 'new-track' }) });
  return items;
}
