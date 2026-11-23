import { describe, expect, it } from 'vitest';
import { decodeMidi, keyToPitch } from '../src/input/liveInput';
import { encodeWav, floatToPcm16, parseWav } from '../src/audio/wav';
import { looksLikeProject, parseProject, safeFileName } from '../src/persistence';

describe('computer keyboard', () => {
  it('maps the home row to a C major scale from middle C', () => {
    const row = ['KeyA', 'KeyS', 'KeyD', 'KeyF', 'KeyG', 'KeyH', 'KeyJ', 'KeyK'];
    expect(row.map((k) => keyToPitch(k, 0))).toEqual([60, 62, 64, 65, 67, 69, 71, 72]);
    expect(keyToPitch('KeyW', 0)).toBe(61);
  });

  it('shifts by octave and ignores other keys', () => {
    expect(keyToPitch('KeyA', 1)).toBe(72);
    expect(keyToPitch('KeyA', -2)).toBe(36);
    expect(keyToPitch('KeyQ', 0)).toBeNull();
    expect(keyToPitch('KeyA', 20)).toBeNull();
  });
});

describe('midi messages', () => {
  it('decodes note on, note off and note on with velocity zero', () => {
    expect(decodeMidi([0x90, 60, 100])).toEqual({ on: true, pitch: 60, velocity: 100 });
    expect(decodeMidi([0x80, 60, 0])).toEqual({ on: false, pitch: 60, velocity: 0 });
    expect(decodeMidi([0x93, 64, 0])?.on).toBe(false);
    expect(decodeMidi([0xb0, 1, 2])).toBeNull();
    expect(decodeMidi([0x90, 60])).toBeNull();
  });
});

describe('wav', () => {
  it('writes a canonical header the parser accepts', () => {
    const wav = encodeWav(Int16Array.from([0, 0, 1000, -1000, 32767, -32768]), 48000, 2);
    expect(wav.length).toBe(44 + 12);
    expect(parseWav(wav)).toEqual({ channels: 2, sampleRate: 48000, bitsPerSample: 16, dataBytes: 12, frames: 3 });
  });

  it('rejects a truncated file', () => {
    const wav = encodeWav(new Int16Array(8), 44100, 2);
    expect(() => parseWav(wav.slice(0, wav.length - 2))).toThrow();
  });

  it('converts floats with clipping', () => {
    expect(floatToPcm16(1)).toBe(32767);
    expect(floatToPcm16(-1)).toBe(-32768);
    expect(floatToPcm16(2)).toBe(32767);
    expect(floatToPcm16(0)).toBe(0);
  });
});

describe('persistence helpers', () => {
  it('accepts project shaped json only', () => {
    const p = { id: 'a', name: 'x', tempo_map: {}, tracks: [] };
    expect(looksLikeProject(p)).toBe(true);
    expect(looksLikeProject({ ...p, tracks: 3 })).toBe(false);
    expect(parseProject(JSON.stringify(p))?.name).toBe('x');
    expect(parseProject('{nope')).toBeNull();
  });

  it('makes safe file names', () => {
    expect(safeFileName(' My song / v2 ', 'wav')).toBe('My-song-v2.wav');
    expect(safeFileName('', 'json')).toBe('project.json');
  });
});
