// Offline render: runs the compiled blob through the wasm engine as fast as possible and
// returns a 16 bit stereo WAV. Same loop as the determinism test, with page yields.
import { config } from '../config';
import { BLOCK_FRAMES, WasmEngine } from '../wasm/engine';
import { readBlobInfo } from '../wasm/blob';
import { encodeWav, floatToPcm16 } from './wav';

const CHANNELS = 2;

export interface RenderResult {
  wav: Uint8Array<ArrayBuffer>;
  frames: number;
  sampleRate: number;
  /** Highest absolute sample before conversion. */
  peak: number;
  clippedFrames: number;
}

const yieldToPage = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

export async function renderBlob(
  module: WebAssembly.Module,
  blob: Uint8Array,
  onProgress?: (fraction: number) => void,
): Promise<RenderResult> {
  const info = readBlobInfo(blob);
  const engine = WasmEngine.fromModule(module);
  engine.load(blob);
  try {
    const total = engine.length;
    if (total === 0) throw new Error('nothing to export');
    const pcm = new Int16Array(total * CHANNELS);
    let peak = 0;
    let done = 0;
    let sinceYield = 0;
    while (done < total) {
      const [l, r] = engine.process(Math.min(BLOCK_FRAMES, total - done));
      for (let i = 0; i < l.length; i++) {
        const a = l[i] ?? 0;
        const b = r[i] ?? 0;
        peak = Math.max(peak, Math.abs(a), Math.abs(b));
        pcm[(done + i) * CHANNELS] = floatToPcm16(a);
        pcm[(done + i) * CHANNELS + 1] = floatToPcm16(b);
      }
      done += l.length;
      sinceYield += l.length;
      if (sinceYield >= config.exportSliceFrames) {
        sinceYield = 0;
        onProgress?.(done / total);
        await yieldToPage();
      }
    }
    onProgress?.(1);
    return {
      wav: encodeWav(pcm, info.sampleRate, CHANNELS),
      frames: total,
      sampleRate: info.sampleRate,
      peak,
      clippedFrames: engine.clipped,
    };
  } finally {
    engine.free();
  }
}
