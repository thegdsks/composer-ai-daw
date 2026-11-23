// Theory reads (D35): types and mappers shared by both backends. The result shapes match
// the wasm exports and the daemon methods theory.scale, theory.chords and theory.pitch.
import type { ScaleName } from '../model';

export interface TheoryKey {
  root: number;
  scale: ScaleName;
}

export type ChordQuality = 'major' | 'minor' | 'diminished' | 'augmented';

export interface TheoryScale {
  root: number;
  scale: ScaleName;
  /** Pitch classes in degree order, 0 = C. */
  pitchClasses: number[];
  mask: number;
}

export interface TheoryChord {
  /** 1 to 7. */
  degree: number;
  name: string;
  roman: string;
  quality: ChordQuality;
  pitchClasses: number[];
  /** Root position MIDI pitches in the requested octave. */
  pitches: number[];
}

export interface TheoryChords {
  root: number;
  scale: ScaleName;
  rootName: string;
  octave: number;
  triads: TheoryChord[];
  sevenths: TheoryChord[];
}

export interface TheoryPitch {
  pitch: number;
  inKey: boolean;
  /** 1 based scale degree, null outside the key. */
  degree: number | null;
  /** Nearest in key pitch, the lower one on a tie. */
  nearest: number | null;
}

/** Daemon and wasm field names. */
export interface RawTheoryScale {
  root: number;
  scale: ScaleName;
  pitch_classes: number[];
  mask: number;
}

export interface RawTheoryChord {
  degree: number;
  name: string;
  roman: string;
  quality: ChordQuality;
  pitch_classes: number[];
  pitches: number[];
}

export interface RawTheoryChords {
  root: number;
  scale: ScaleName;
  root_name: string;
  octave: number;
  triads: RawTheoryChord[];
  sevenths: RawTheoryChord[];
}

export interface RawTheoryPitch {
  pitch: number;
  in_key: boolean;
  degree: number | null;
  nearest: number | null;
}

export function toTheoryScale(r: RawTheoryScale): TheoryScale {
  return { root: r.root, scale: r.scale, pitchClasses: r.pitch_classes, mask: r.mask };
}

function toChord(c: RawTheoryChord): TheoryChord {
  return {
    degree: c.degree,
    name: c.name,
    roman: c.roman,
    quality: c.quality,
    pitchClasses: c.pitch_classes,
    pitches: c.pitches,
  };
}

export function toTheoryChords(r: RawTheoryChords): TheoryChords {
  return {
    root: r.root,
    scale: r.scale,
    rootName: r.root_name,
    octave: r.octave,
    triads: r.triads.map(toChord),
    sevenths: r.sevenths.map(toChord),
  };
}

export function toTheoryPitch(r: RawTheoryPitch): TheoryPitch {
  return { pitch: r.pitch, inKey: r.in_key, degree: r.degree, nearest: r.nearest };
}
