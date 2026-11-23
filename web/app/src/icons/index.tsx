// Icons: our own set from @thesvg/ui-react (MIT, on the public registry) plus a few
// local ones for things the set does not cover. No Lucide or Phosphor.
import type { SVGProps } from 'react';

export { PlayIcon } from '@thesvg/ui-react/play';
export { PauseIcon } from '@thesvg/ui-react/pause';
export { SquareIcon as StopIcon } from '@thesvg/ui-react/square';
export { CopyIcon } from '@thesvg/ui-react/copy';
export { FilesIcon as RepeatNIcon } from '@thesvg/ui-react/files';
export { UndoIcon } from '@thesvg/ui-react/undo';
export { RedoIcon } from '@thesvg/ui-react/redo';
export { PlusIcon } from '@thesvg/ui-react/plus';
export { MinusIcon } from '@thesvg/ui-react/minus';
export { TrashIcon } from '@thesvg/ui-react/trash';
export { DownloadIcon } from '@thesvg/ui-react/download';
export { UploadIcon } from '@thesvg/ui-react/upload';
export { SaveIcon } from '@thesvg/ui-react/save';
export { KeyboardIcon } from '@thesvg/ui-react/keyboard';
export { SettingsIcon } from '@thesvg/ui-react/settings';
export { SpeakerIcon } from '@thesvg/ui-react/speaker';
export { FilmIcon } from '@thesvg/ui-react/film';
export { MoonIcon } from '@thesvg/ui-react/moon';
export { HeadphonesIcon } from '@thesvg/ui-react/headphones';
export { ClockIcon } from '@thesvg/ui-react/clock';
export { SearchIcon } from '@thesvg/ui-react/search';
export { XIcon } from '@thesvg/ui-react/x';
export { ChevronDownIcon } from '@thesvg/ui-react/chevron-down';
export { FileIcon } from '@thesvg/ui-react/file';
export { SlidersVerticalIcon } from '@thesvg/ui-react/sliders-vertical';
export { RepeatIcon as LoopIcon } from '@thesvg/ui-react/repeat';
export { LocateFixedIcon as FollowIcon } from '@thesvg/ui-react/locate-fixed';
export { GripVerticalIcon as GripIcon } from '@thesvg/ui-react/grip-vertical';
export { EllipsisVerticalIcon as MoreIcon } from '@thesvg/ui-react/ellipsis-vertical';
export { ChevronUpIcon } from '@thesvg/ui-react/chevron-up';
export { PowerIcon } from '@thesvg/ui-react/power';
export { AudioWaveformIcon as EffectsIcon } from '@thesvg/ui-react/audio-waveform';

type LocalProps = SVGProps<SVGSVGElement> & { size?: number };

function Base({ size = 24, children, ...rest }: LocalProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...rest}
    >
      {children}
    </svg>
  );
}

/** A filled dot, for Record. */
export function RecordIcon(props: LocalProps) {
  return (
    <Base {...props}>
      <circle cx="12" cy="12" r="6" fill="currentColor" />
    </Base>
  );
}

/** Five piano keys, for the MIDI keyboard control. */
export function MidiIcon(props: LocalProps) {
  return (
    <Base {...props}>
      <rect x="3" y="5" width="18" height="14" rx="1.5" />
      <path d="M8 5v14M12 5v14M16 5v14" />
      <path d="M6.5 5v7M10.5 5v7M14.5 5v7M18.5 5v7" strokeWidth={3} />
    </Base>
  );
}

/** A note head on a stem, for empty states and the sound slot. */
export function NoteIcon(props: LocalProps) {
  return (
    <Base {...props}>
      <ellipse cx="8" cy="17" rx="3.5" ry="2.75" />
      <path d="M11.5 17V5l7 2" />
    </Base>
  );
}

/** Magnifier with a plus, zoom in. */
export function ZoomInIcon(props: LocalProps) {
  return (
    <Base {...props}>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="M15 15l5 5M10.5 8v5M8 10.5h5" />
    </Base>
  );
}

/** Magnifier with a minus, zoom out. */
export function ZoomOutIcon(props: LocalProps) {
  return (
    <Base {...props}>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="M15 15l5 5M8 10.5h5" />
    </Base>
  );
}

/** A metronome: tapered body with the swinging arm. */
export function MetronomeIcon(props: LocalProps) {
  return (
    <Base {...props}>
      <path d="M8 21h8l-2-17h-4z" />
      <path d="M12 16L17 6" />
      <circle cx="15" cy="9.5" r="1" />
    </Base>
  );
}

/** Scissors, split a clip at the playhead. */
export function ScissorsIcon(props: LocalProps) {
  return (
    <Base {...props}>
      <circle cx="6" cy="6.5" r="2.5" />
      <circle cx="6" cy="17.5" r="2.5" />
      <path d="M8 8l12 9M8 16L20 7" />
    </Base>
  );
}
