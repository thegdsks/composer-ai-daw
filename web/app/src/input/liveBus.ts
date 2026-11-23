// One place where a live note goes in: the computer keys, MIDI, the on screen keys and the chord
// pads all call these, and the app registers what sounds it, lights the roll and feeds the take.
import type { LiveSink } from './liveInput';

const noop: LiveSink = { noteOn: () => undefined, noteOff: () => undefined };
let sink: LiveSink = noop;

export function setLiveSink(next: LiveSink): () => void {
  sink = next;
  return () => {
    if (sink === next) sink = noop;
  };
}

export function liveOn(pitch: number, velocity: number): void {
  sink.noteOn(pitch, velocity);
}

export function liveOff(pitch: number): void {
  sink.noteOff(pitch);
}
