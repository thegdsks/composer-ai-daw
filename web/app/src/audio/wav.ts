// 16 bit PCM WAV writer and a header parser used by tests and the export check.

const HEADER_BYTES = 44;
const BITS = 16;

export interface WavInfo {
  channels: number;
  sampleRate: number;
  bitsPerSample: number;
  dataBytes: number;
  frames: number;
}

export function wavHeader(frames: number, sampleRate: number, channels: number): Uint8Array {
  const dataBytes = frames * channels * (BITS / 8);
  const buf = new ArrayBuffer(HEADER_BYTES);
  const v = new DataView(buf);
  const tag = (o: number, s: string): void => {
    for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i));
  };
  tag(0, 'RIFF');
  v.setUint32(4, 36 + dataBytes, true);
  tag(8, 'WAVE');
  tag(12, 'fmt ');
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, channels, true);
  v.setUint32(24, sampleRate, true);
  v.setUint32(28, sampleRate * channels * (BITS / 8), true);
  v.setUint16(32, channels * (BITS / 8), true);
  v.setUint16(34, BITS, true);
  tag(36, 'data');
  v.setUint32(40, dataBytes, true);
  return new Uint8Array(buf);
}

/** Interleaved samples to a complete WAV file. */
export function encodeWav(pcm: Int16Array, sampleRate: number, channels: number): Uint8Array<ArrayBuffer> {
  const frames = pcm.length / channels;
  const out = new Uint8Array(HEADER_BYTES + pcm.byteLength);
  out.set(wavHeader(frames, sampleRate, channels), 0);
  out.set(new Uint8Array(pcm.buffer, pcm.byteOffset, pcm.byteLength), HEADER_BYTES);
  return out;
}

export function floatToPcm16(x: number): number {
  const c = Math.max(-1, Math.min(1, x));
  return Math.round(c < 0 ? c * 0x8000 : c * 0x7fff);
}

export function parseWav(bytes: Uint8Array): WavInfo {
  if (bytes.length < HEADER_BYTES) throw new Error('wav too short');
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tag = (o: number): string => String.fromCharCode(...bytes.subarray(o, o + 4));
  if (tag(0) !== 'RIFF' || tag(8) !== 'WAVE' || tag(12) !== 'fmt ' || tag(36) !== 'data') {
    throw new Error('not a canonical wav');
  }
  if (v.getUint16(20, true) !== 1) throw new Error('not PCM');
  const channels = v.getUint16(22, true);
  const bitsPerSample = v.getUint16(34, true);
  const dataBytes = v.getUint32(40, true);
  if (v.getUint32(4, true) !== 36 + dataBytes || bytes.length !== HEADER_BYTES + dataBytes) {
    throw new Error('wav sizes disagree');
  }
  return {
    channels,
    sampleRate: v.getUint32(24, true),
    bitsPerSample,
    dataBytes,
    frames: dataBytes / (channels * (bitsPerSample / 8)),
  };
}
