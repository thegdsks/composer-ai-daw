// Instrument and waveform icons. Bell comes from @thesvg/ui-react; the rest are drawn here
// in the same 24 grid, 1.75 stroke, currentColor style because the set has no instrument icons yet.
import { createElement, type ComponentType, type SVGProps } from 'react';
import { BellIcon } from '@thesvg/ui-react/bell';
import type { SoundEntry } from '../backend/types';
import { INSTRUMENT_SAW, INSTRUMENT_SINE, type Track } from '../model';
import { categoryOf } from '../sounds/catalogue';
import { NoteIcon } from './index';

type Props = SVGProps<SVGSVGElement> & { size?: number };
export type InstrumentIcon = ComponentType<Props>;

function Base({ size = 16, children, ...rest }: Props) {
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

/** Three white keys with two black keys, for piano and keys. */
export function PianoIcon(props: Props) {
  return (
    <Base {...props}>
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <path d="M9 12v7M15 12v7" />
      <rect x="7.5" y="5" width="3" height="7" fill="currentColor" />
      <rect x="13.5" y="5" width="3" height="7" fill="currentColor" />
    </Base>
  );
}

/** A violin body with its neck, for bowed strings. */
export function StringsIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M12 9c-2 0-3 1-3 2.5 0 1 .8 1.5.8 2.2S9 15 9 16.5C9 18.5 10.5 21 12 21s3-2.5 3-4.5c0-1.5-.8-1.8-.8-2.8s.8-1.2.8-2.2C15 10 14 9 12 9Z" />
      <path d="M12 9V3M10.5 3h3" />
    </Base>
  );
}

/** A soft cloud, for pads. */
export function PadIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M7.5 18a4 4 0 0 1-.6-7.95 5.5 5.5 0 0 1 10.6 1.45A3.3 3.3 0 0 1 17 18Z" />
    </Base>
  );
}

/** A woofer cone, for bass. */
export function BassIcon(props: Props) {
  return (
    <Base {...props}>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="4" />
      <circle cx="12" cy="12" r="0.75" fill="currentColor" />
    </Base>
  );
}

/** A string pulled to a point over its resting line, for plucks. */
export function PluckIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M4 6v12M20 6v12" />
      <path d="M4 12 12 5l8 7" />
      <path d="M4 12h16" strokeDasharray="1 3" />
      <circle cx="12" cy="5" r="1" fill="currentColor" />
    </Base>
  );
}

/** Three singers, for choir. */
export function ChoirIcon(props: Props) {
  return (
    <Base {...props}>
      <circle cx="12" cy="8" r="2.75" />
      <path d="M7.5 19v-1.5a4.5 4.5 0 0 1 9 0V19" />
      <path d="M3 18v-1a3 3 0 0 1 3-3M21 18v-1a3 3 0 0 0-3-3" />
    </Base>
  );
}

/** Narrow pulses, for the pulse voice. */
export function PulseIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M2 17h4V7h2v10h5V7h2v10h5" />
    </Base>
  );
}

export function SineWaveIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M2 12c2.5-9 7.5-9 10 0s7.5 9 10 0" />
    </Base>
  );
}

export function SawWaveIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M3 17 10 7v10l7-10v10" />
    </Base>
  );
}

export function SquareWaveIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M3 16V8h6v8h6V8h6" />
    </Base>
  );
}

export function TriangleWaveIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M2 16 6.5 8 11 16l4.5-8L20 16" />
    </Base>
  );
}

export function NoiseIcon(props: Props) {
  return (
    <Base {...props}>
      <path d="M4 9v6M7 5v14M10 10v5M13 7v10M16 11v3M19 8v8" />
    </Base>
  );
}

/** Used for anything without a mapping (sample packs, custom sounds). */
export const FALLBACK_ICON: InstrumentIcon = NoteIcon;

const BY_KEY: Record<string, InstrumentIcon> = {
  // sound categories
  keys: PianoIcon,
  strings: StringsIcon,
  pads: PadIcon,
  bass: BassIcon,
  plucks: PluckIcon,
  choir: ChoirIcon,
  bells: BellIcon,
  pulse: PulseIcon,
  // built in voices
  'builtin-soft-piano': PianoIcon,
  'builtin-warm-pad': PadIcon,
  'builtin-warm-strings': StringsIcon,
  'builtin-pluck': PluckIcon,
  'builtin-sub-bass': BassIcon,
  'builtin-choir-pad': ChoirIcon,
  'builtin-bell': BellIcon,
  'builtin-soft-pulse': PulseIcon,
  'builtin-electric-piano': PianoIcon,
  'builtin-mallet': BellIcon,
  'builtin-guitar-pluck': PluckIcon,
  'builtin-brass-pad': PadIcon,
  'builtin-flute': PulseIcon,
  'builtin-organ': PianoIcon,
  'builtin-bass-pluck': BassIcon,
  'builtin-lead': SawWaveIcon,
  // plain waveforms
  sine: SineWaveIcon,
  saw: SawWaveIcon,
  square: SquareWaveIcon,
  triangle: TriangleWaveIcon,
  noise: NoiseIcon,
  [INSTRUMENT_SINE]: SineWaveIcon,
  [INSTRUMENT_SAW]: SawWaveIcon,
};

/** Icon for a sound entry, a category, a built in voice id, a waveform name or an instrument id. Never throws. */
export function instrumentIcon(soundOrCategory: SoundEntry | string | null | undefined): InstrumentIcon {
  if (!soundOrCategory) return FALLBACK_ICON;
  if (typeof soundOrCategory === 'string') return BY_KEY[soundOrCategory.toLowerCase()] ?? FALLBACK_ICON;
  return BY_KEY[soundOrCategory.id] ?? BY_KEY[categoryOf(soundOrCategory).toLowerCase()] ?? FALLBACK_ICON;
}

/** Icon for what a track plays: its catalogue sound, else its plain waveform. */
export function trackIcon(track: Track, entries: readonly SoundEntry[]): InstrumentIcon {
  if (track.sound) {
    const e = entries.find((x) => x.id === track.sound);
    return e ? instrumentIcon(e) : instrumentIcon(track.sound);
  }
  return instrumentIcon(track.instrument);
}

/** Renders the icon for a sound, category or track. A component, so callers never build one during render. */
export function InstrumentGlyph(
  props: Props & { of: SoundEntry | string | null | undefined; track?: Track; entries?: readonly SoundEntry[] },
) {
  const { of, track, entries, ...rest } = props;
  const icon = track ? trackIcon(track, entries ?? []) : instrumentIcon(of);
  return createElement(icon, rest);
}
