// Messages between the page and the audio worklet.

/** Metronome and loop, in sample frames of the song. Never part of the project or its hash. */
export interface TransportPlan {
  loop: { on: boolean; startFrame: number; endFrame: number };
  metronome: boolean;
  /** Frame of every beat, ascending, and whether it opens a bar. */
  beats: Float64Array;
  accents: Uint8Array;
}

/** Clicks played before the song moves, as frame offsets from the press of Play. */
export interface CountIn {
  frames: Float64Array;
  accents: Uint8Array;
  totalFrames: number;
}

export type ToWorklet =
  | {
      type: 'load';
      blob: Uint8Array;
      /** Keep the playhead by rendering silently up to the old position. */
      keepPosition: boolean;
      maxSeekFrames: number;
    }
  | { type: 'play'; countIn: CountIn | null }
  | { type: 'pause' }
  /** Moves the playhead; the worklet prepares a spare engine so the audio never stalls. */
  | { type: 'seek'; frame: number; id: number }
  | { type: 'plan'; plan: TransportPlan }
  | { type: 'note'; track: number; pitch: number; velocity: number; on: boolean }
  /** Replaces the preview engine (sound browser audition); its notes use track 0. */
  | { type: 'preview-load'; blob: Uint8Array }
  | { type: 'preview-note'; pitch: number; velocity: number; on: boolean }
  /** Loads a web plugin (the bytes of one .wclap file) as an insert on the master output. */
  | { type: 'wclap-load'; bytes: Uint8Array; id: number }
  | { type: 'wclap-param'; param: number; value: number }
  /** Kill switch: drops the plugin, the audio is untouched. */
  | { type: 'wclap-unload' };

/** What the page knows about the loaded plugin; measured in the worklet. */
export interface PluginInfo {
  id: string;
  name: string;
  params: { id: number; name: string; min: number; max: number; def: number }[];
  latencyFrames: number;
}

export type FromWorklet =
  | { type: 'ready' }
  | {
      type: 'pos';
      frames: number;
      /** Audio clock (seconds) at the end of the block `frames` was read after; pairs the position with the clock for recording. */
      at: number;
      lengthFrames: number;
      playing: boolean;
      /** True while the count in plays and the song has not moved yet. */
      counting: boolean;
      /** Highest absolute sample since the previous report. */
      peak: number;
      clipped: number;
      /** Seeks finished, loop wraps done and clicks started since the worklet began. */
      seeks: number;
      wraps: number;
      clicks: number;
      /** Loudest click sample so far. */
      clickPeak: number;
      /** Per track peak since the previous report (post fader and pan, blob track order). Absent from older workers. */
      trackPeaks?: Float32Array;
    }
  | { type: 'ended' }
  | { type: 'wclap-loaded'; id: number; info: PluginInfo }
  | { type: 'wclap-failed'; id: number; code: string; message: string }
  | {
      type: 'wclap-stats';
      inPeak: number;
      outPeak: number;
      blocks: number;
      avgBlockUs: number;
      maxBlockUs: number;
      overruns: number;
      droppedEvents: number;
      bypassed: string | null;
    }
  | { type: 'error'; message: string };

export interface WorkletOptions {
  wasm: ArrayBuffer;
  positionReportFrames: number;
  clickMs: number;
  clickGain: number;
  clickHz: number;
  clickAccentHz: number;
  seekBudgetFrames: number;
  wclap: { maxFileBytes: number; maxMemoryPages: number; maxEvents: number; budgetShare: number; overrunLimit: number; reportBlocks: number };
}
