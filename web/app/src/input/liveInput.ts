// Live note input: the computer keyboard as a piano (home row) and Web MIDI when present.
import { config } from '../config';
import { getLiveOctave, setLiveOctave } from './liveOctave';

export interface LiveSink {
  noteOn(pitch: number, velocity: number): void;
  noteOff(pitch: number): void;
}

/** Semitone offset from the base note, by physical key so layouts other than QWERTY work. */
const KEY_OFFSETS: Readonly<Record<string, number>> = {
  KeyA: 0,
  KeyW: 1,
  KeyS: 2,
  KeyE: 3,
  KeyD: 4,
  KeyF: 5,
  KeyT: 6,
  KeyG: 7,
  KeyY: 8,
  KeyH: 9,
  KeyU: 10,
  KeyJ: 11,
  KeyK: 12,
  KeyO: 13,
  KeyL: 14,
  KeyP: 15,
  Semicolon: 16,
};

export function keyToPitch(code: string, octave: number): number | null {
  const off = KEY_OFFSETS[code];
  if (off === undefined) return null;
  const pitch = config.keyboardBaseNote + octave * 12 + off;
  return pitch >= 0 && pitch <= 127 ? pitch : null;
}

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  // A canvas marked data-own-keys uses the letter keys for its own shortcuts.
  return target.isContentEditable || ['INPUT', 'SELECT', 'TEXTAREA'].includes(target.tagName) || target.closest('[data-own-keys]') !== null;
}

/** Wires the piano keys; returns a cleanup. Octave keys: Z down, X up. Presses are ignored while `enabled` says no. */
export function attachComputerKeyboard(sink: LiveSink, onOctave?: (o: number) => void, enabled: () => boolean = () => true): () => void {
  // Remember the pitch each key started so a late octave change cannot orphan a note.
  const held = new Map<string, number>();

  const down = (e: KeyboardEvent): void => {
    if (e.repeat || e.ctrlKey || e.metaKey || e.altKey || isTyping(e.target) || !enabled()) return;
    if (e.code === 'KeyZ' || e.code === 'KeyX') {
      onOctave?.(setLiveOctave(getLiveOctave() + (e.code === 'KeyX' ? 1 : -1)));
      return;
    }
    const pitch = keyToPitch(e.code, getLiveOctave());
    if (pitch === null || held.has(e.code)) return;
    held.set(e.code, pitch);
    sink.noteOn(pitch, config.liveVelocity);
  };
  const up = (e: KeyboardEvent): void => {
    const pitch = held.get(e.code);
    if (pitch === undefined) return;
    held.delete(e.code);
    sink.noteOff(pitch);
  };
  const blur = (): void => {
    for (const p of held.values()) sink.noteOff(p);
    held.clear();
  };
  window.addEventListener('keydown', down);
  window.addEventListener('keyup', up);
  window.addEventListener('blur', blur);
  return () => {
    window.removeEventListener('keydown', down);
    window.removeEventListener('keyup', up);
    window.removeEventListener('blur', blur);
    blur();
  };
}

const NOTE_OFF = 0x80;
const NOTE_ON = 0x90;
const STATUS_MASK = 0xf0;

/** Decodes a raw MIDI message into a note event, or null for anything else. */
export function decodeMidi(data: ArrayLike<number>): { on: boolean; pitch: number; velocity: number } | null {
  if (data.length < 3) return null;
  const status = (data[0] ?? 0) & STATUS_MASK;
  const pitch = data[1] ?? 0;
  const velocity = data[2] ?? 0;
  if (status === NOTE_ON && velocity > 0) return { on: true, pitch, velocity };
  if (status === NOTE_OFF || status === NOTE_ON) return { on: false, pitch, velocity };
  return null;
}

export function midiSupported(): boolean {
  return typeof navigator !== 'undefined' && typeof navigator.requestMIDIAccess === 'function';
}

export interface MidiConnection {
  inputs: number;
  close(): void;
}

/** Asks for MIDI access (shows the browser prompt) and routes every input to the sink. */
export async function connectMidi(sink: LiveSink): Promise<MidiConnection> {
  if (!midiSupported()) throw new Error('Web MIDI is not available in this browser');
  const access = await navigator.requestMIDIAccess();
  const attached = new Set<MIDIInput>();
  const handler = (e: MIDIMessageEvent): void => {
    if (!e.data) return;
    const m = decodeMidi(e.data);
    if (!m) return;
    if (m.on) sink.noteOn(m.pitch, m.velocity);
    else sink.noteOff(m.pitch);
  };
  const attach = (): void => {
    for (const input of access.inputs.values()) {
      if (attached.has(input)) continue;
      input.addEventListener('midimessage', handler);
      attached.add(input);
    }
  };
  attach();
  access.onstatechange = attach;
  return {
    get inputs() {
      return attached.size;
    },
    close() {
      for (const input of attached) input.removeEventListener('midimessage', handler);
      attached.clear();
      access.onstatechange = null;
    },
  };
}
