// The eight built in voices as a catalogue, for backends that have no catalogue file
// (the in-page editor). Ids, names and kind must match sounds/catalogue.yaml; a test checks it.
import type { SoundCatalogue, SoundEntry } from '../backend/types';

const LICENCE = 'Original project code, generated at run time, no third party audio, presets or samples';

function voice(id: string, name: string, instruments: string): SoundEntry {
  return { id, name, kind: 'builtin', licence: LICENCE, instruments, bundle: 'yes', attribution_required: 'no' };
}

export const BUILTIN_ENTRIES: SoundEntry[] = [
  voice('builtin-soft-piano', 'Soft Piano', 'piano with a long natural decay'),
  voice('builtin-warm-pad', 'Warm Pad', 'wide pad with a slow breathing filter'),
  voice('builtin-warm-strings', 'Warm Strings', 'bowed strings with a slow attack and gentle vibrato'),
  voice('builtin-pluck', 'Pluck', 'short plucked tone that closes after the attack'),
  voice('builtin-sub-bass', 'Sub Bass', 'round low bass with soft harmonics'),
  voice('builtin-choir-pad', 'Choir Pad', 'breathy ah choir with a slow swell'),
  voice('builtin-bell', 'Bell', 'bright bell with long ringing partials'),
  voice('builtin-soft-pulse', 'Soft Pulse', 'mellow pulse wave for a lead or an arp'),
  voice('builtin-electric-piano', 'Electric Piano', 'tine piano with a soft bark and a slow tremolo'),
  voice('builtin-mallet', 'Mallet', 'marimba like bar, short and woody'),
  voice('builtin-guitar-pluck', 'Guitar Pluck', 'plucked string with a little pick noise'),
  voice('builtin-brass-pad', 'Brass Pad', 'brassy swell that opens up'),
  voice('builtin-flute', 'Flute', 'breathy flute with a late vibrato'),
  voice('builtin-organ', 'Organ', 'drawbar organ with a slow rotating speaker'),
  voice('builtin-bass-pluck', 'Bass Pluck', 'round plucked bass that closes after the attack'),
  voice('builtin-lead', 'Lead', 'bright lead with vibrato that fades in'),
];

export function builtinCatalogue(): SoundCatalogue {
  return { entries: BUILTIN_ENTRIES.map((e) => ({ ...e })), gaps: [] };
}
