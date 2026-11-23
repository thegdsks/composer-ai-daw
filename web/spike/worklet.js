import { WasmEngine } from './engine.mjs';

const STATUS_EVERY_FRAMES = 24000;

class ComposerProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const { wasm, sabBuffer } = options.processorOptions;
    this.engine = WasmEngine.fromBytes(wasm);
    this.blob = this.engine.demoBlob(sampleRate);
    this.engine.load(this.blob);
    this.sab = sabBuffer ? new Int32Array(sabBuffer) : null;
    this.rendered = 0;
    this.sinceStatus = 0;
    this.peak = 0;
    this.maxPeak = 0;
    this.loops = 0;
    this.rejected = 0;
    this.reloads = 0;
    this.port.onmessage = (e) => {
      const m = e.data;
      if (m.type === 'note' && !this.engine.note(m.track, m.pitch, m.velocity ?? 100, m.on)) {
        this.rejected++;
      }
      // A new compiled project from the editor replaces the looping blob.
      if (m.type === 'load') {
        this.blob = m.blob;
        this.engine.load(this.blob);
        this.reloads++;
      }
    };
  }

  process(_inputs, outputs) {
    const out = outputs[0];
    const n = out[0].length;
    // Loop the demo: restart when the offline length has been rendered.
    if (this.engine.position >= this.engine.length) {
      this.engine.load(this.blob);
      this.loops++;
    }
    const [l, r] = this.engine.process(n);
    out[0].set(l);
    if (out[1]) out[1].set(r);
    for (let i = 0; i < n; i++) {
      const a = Math.max(Math.abs(l[i]), Math.abs(r[i]));
      if (a > this.peak) this.peak = a;
    }
    this.rendered += n;
    this.sinceStatus += n;
    if (this.sinceStatus >= STATUS_EVERY_FRAMES) {
      this.sinceStatus = 0;
      if (this.peak > this.maxPeak) this.maxPeak = this.peak;
      this.port.postMessage({
        type: 'status',
        framesRendered: this.rendered,
        enginePosition: this.engine.position,
        clippedFrames: this.engine.clipped,
        peak: this.peak,
        maxPeak: this.maxPeak,
        loops: this.loops,
        rejectedLiveNotes: this.rejected,
        reloads: this.reloads,
        worklet: {
          sampleRate,
          currentTime,
          crossOriginIsolated: typeof crossOriginIsolated === 'undefined' ? null : crossOriginIsolated,
          sharedArrayBuffer: typeof SharedArrayBuffer !== 'undefined',
          sabCounterSeenByWorklet: this.sab ? Atomics.load(this.sab, 0) : null,
        },
      });
      this.peak = 0;
    }
    return true;
  }
}

registerProcessor('composer-engine', ComposerProcessor);
