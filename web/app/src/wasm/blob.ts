// Helpers over engine blob v1 and v2 (header CMPW, 32 bytes; tracks 12 bytes in v1 and 16 in
// v2, where the record carries the channel mix; events 16 bytes).

const HEADER_BYTES = 32;
const TRACK_BYTES_V1 = 12;
const TRACK_BYTES_V2 = 16;
const VERSION_OFFSET = 4;
const EVENT_COUNT_OFFSET = 20;

export interface BlobInfo {
  version: number;
  sampleRate: number;
  trackCount: number;
  eventCount: number;
  lengthFrames: number;
}

export function readBlobInfo(blob: Uint8Array): BlobInfo {
  if (blob.length < HEADER_BYTES) throw new Error('blob too short');
  const v = new DataView(blob.buffer, blob.byteOffset, blob.byteLength);
  return {
    version: v.getUint32(VERSION_OFFSET, true),
    sampleRate: v.getUint32(8, true),
    trackCount: v.getUint32(16, true),
    eventCount: v.getUint32(20, true),
    lengthFrames: Number(v.getBigUint64(24, true)),
  };
}

const EVENT_BYTES = 16;
const LAST_VERSION = 7;

function trackBytes(version: number): number {
  if (version === 1) return TRACK_BYTES_V1;
  if (version >= 2 && version <= LAST_VERSION) return TRACK_BYTES_V2;
  throw new Error(`unknown blob version ${version}`);
}

/**
 * Same tracks and sections (banks, master, loops, lanes, effect chains), no scheduled events: an
 * engine that only plays live notes. Versions 3 to 7 keep their sections after the events.
 */
export function liveOnlyBlob(blob: Uint8Array): Uint8Array {
  const info = readBlobInfo(blob);
  const tracksEnd = HEADER_BYTES + info.trackCount * trackBytes(info.version);
  const eventsEnd = tracksEnd + info.eventCount * EVENT_BYTES;
  const out = new Uint8Array(tracksEnd + Math.max(0, blob.length - eventsEnd));
  out.set(blob.subarray(0, tracksEnd));
  out.set(blob.subarray(Math.min(eventsEnd, blob.length)), tracksEnd);
  new DataView(out.buffer).setUint32(EVENT_COUNT_OFFSET, 0, true);
  return out;
}

/** Everything but the events, to tell whether a new blob changes the instruments, the mix or the effects. */
export function trackTable(blob: Uint8Array): Uint8Array {
  return liveOnlyBlob(blob).subarray(HEADER_BYTES);
}

export function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}
