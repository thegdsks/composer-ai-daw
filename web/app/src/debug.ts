// Read-only measurements exposed on window.composerDebug so automation and the owner can
// check audio and drawing without parsing the page. Nothing here changes behavior.
import { config } from './config';
import type { AudioStatus } from './audio/audioEngine';
import type { TransportPrefs } from './backend/types';

export interface FrameStats {
  frames: number;
  lastMs: number;
  avgMs: number;
  p95Ms: number;
  maxMs: number;
}

const SOUND_POLL_MS = 20;
const DUMP_MS = 250;

/** Rolling window of draw times for one canvas. */
class FrameLog {
  samples: number[] = [];
  total = 0;
  maxMs = 0;

  record(ms: number): void {
    this.samples.push(ms);
    if (this.samples.length > config.frameSamples) this.samples.shift();
    this.total++;
    this.maxMs = Math.max(this.maxMs, ms);
  }

  reset(): void {
    this.samples.length = 0;
    this.total = 0;
    this.maxMs = 0;
  }

  stats(): FrameStats {
    const sorted = [...this.samples].sort((a, b) => a - b);
    const sum = sorted.reduce((a, b) => a + b, 0);
    return {
      frames: this.total,
      lastMs: this.samples[this.samples.length - 1] ?? 0,
      avgMs: sorted.length ? sum / sorted.length : 0,
      p95Ms: sorted[Math.floor(sorted.length * 0.95)] ?? 0,
      maxMs: this.maxMs,
    };
  }
}

const rollLog = new FrameLog();
const arrangementLog = new FrameLog();
let longTasks = 0;
let longestTaskMs = 0;

let editCount = 0;
let lastEditMs = 0;
let maxEditMs = 0;

/** Time from sending a command to the refreshed project being in the store. */
export function recordEdit(ms: number): void {
  editCount++;
  lastEditMs = ms;
  maxEditMs = Math.max(maxEditMs, ms);
}

const LOG_MAX = 50;
const commandLog: string[] = [];

/** Keeps the labels of the last LOG_MAX edits, oldest first. */
export function recordCommand(label: string): void {
  commandLog.push(label);
  if (commandLog.length > LOG_MAX) commandLog.shift();
}

let actions = 0;
let actionsToFirstSound: number | null = null;
let firstSoundMs: number | null = null;
let firstGestureMs: number | null = null;
let soundTimer: ReturnType<typeof setInterval> | null = null;

/** Counts user gestures and stamps the first moment the engine reports a nonzero peak. */
function watchFirstSound(audio: () => AudioStatus): void {
  const count = (): void => {
    firstGestureMs ??= performance.now();
    if (firstSoundMs === null) actions++;
  };
  window.addEventListener('pointerdown', count, true);
  window.addEventListener('keydown', count, true);
  soundTimer = setInterval(() => {
    if (audio().maxPeak <= 0) return;
    // performance.now() counts from navigation start, so this is time since the page was requested.
    firstSoundMs = performance.now();
    actionsToFirstSound = actions;
    if (soundTimer) clearInterval(soundTimer);
    soundTimer = null;
  }, SOUND_POLL_MS);
}

export function recordFrame(ms: number): void {
  rollLog.record(ms);
}

/** Draw time of one arrangement frame (the same rolling window as the roll). */
export function recordArrangementFrame(ms: number): void {
  arrangementLog.record(ms);
}

export function resetFrameStats(): void {
  rollLog.reset();
  arrangementLog.reset();
  longTasks = 0;
  longestTaskMs = 0;
}

export function frameStats(): FrameStats {
  return rollLog.stats();
}

function observeLongTasks(): void {
  try {
    const supported = PerformanceObserver.supportedEntryTypes;
    const type = supported.includes('long-animation-frame') ? 'long-animation-frame' : 'longtask';
    if (!supported.includes(type)) return;
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        longTasks++;
        longestTaskMs = Math.max(longestTaskMs, e.duration);
      }
    }).observe({ type, buffered: true });
  } catch {
    // Not available in this browser.
  }
}

/** Where the arrangement draws things, in page pixels, so a driver can aim pointer gestures. */
export interface ArrangementProbe {
  pxPerTick: number;
  scrollTick: number;
  canvas: { x: number; y: number; width: number; height: number };
  rulerHeight: number;
  rowHeight: number;
  clips: { id: string; track: string; lane: number; start: number; length: number; x: number; y: number; width: number; height: number }[];
  selected: string[];
}

let arrangementProbe: (() => ArrangementProbe) | null = null;

/** The arrangement registers itself here; pass null on teardown. */
export function setArrangementProbe(fn: (() => ArrangementProbe) | null): void {
  arrangementProbe = fn;
}

export interface DebugSources {
  audio: () => AudioStatus;
  project: () => {
    tracks: number;
    regions: number;
    notes: number;
    hash: string;
    backend: string;
    undoDepth: number;
    selectedNotes: number;
    /** Catalogue sound id per track, null when a track plays its basic instrument. */
    sounds: (string | null)[];
    /** Markers and tempo points by tick, for e2e checks of the timeline lanes. */
    timeline?: { markers: Record<string, string>; tempos: Record<string, number>; loops: number };
    /** Loop, metronome, count in and follow as the session holds them. */
    transport: TransportPrefs;
  };
}

export interface LastExport {
  name: string;
  bytes: number;
  header: unknown;
  peak: number;
  renderMs: number;
}

export interface ComposerDebug {
  crossOriginIsolated: boolean;
  audio: AudioStatus;
  project: ReturnType<DebugSources['project']>;
  roll: FrameStats;
  /** Arrangement canvas draw times, same window as `roll`. */
  arrangementDraw: FrameStats;
  /** performance.now() when the project was ready and the debug object installed (ms since navigation start). */
  readyMs: number;
  /** JS heap in use in bytes (Chromium only), null elsewhere. */
  heapBytes: number | null;
  longTasks: { count: number; longestMs: number };
  resetStats(): void;
  edits: { count: number; lastMs: number; maxMs: number };
  lastExport: LastExport | null;
  /** Milliseconds from navigation start to the first audible output, null until then. */
  firstSoundMs: number | null;
  /** Pointer and key gestures made before that first sound, null until then. */
  actionsToFirstSound: number | null;
  /** Navigation start to the first pointer or key gesture; firstSoundMs minus this is press to sound. */
  firstGestureMs: number | null;
  commandLog: string[];
  undoDepth: number;
  arrangement: ArrangementProbe | null;
}

export function installDebug(sources: DebugSources): ComposerDebug {
  observeLongTasks();
  const readyMs = performance.now();
  const dbg: ComposerDebug = {
    readyMs,
    get crossOriginIsolated() {
      return self.crossOriginIsolated;
    },
    get audio() {
      return { ...sources.audio() };
    },
    get project() {
      return sources.project();
    },
    get roll() {
      return frameStats();
    },
    get arrangementDraw() {
      return arrangementLog.stats();
    },
    get heapBytes() {
      const m = (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory;
      return m ? m.usedJSHeapSize : null;
    },
    get longTasks() {
      return { count: longTasks, longestMs: longestTaskMs };
    },
    get edits() {
      return { count: editCount, lastMs: lastEditMs, maxMs: maxEditMs };
    },
    get firstSoundMs() {
      return firstSoundMs;
    },
    get firstGestureMs() {
      return firstGestureMs;
    },
    get actionsToFirstSound() {
      return actionsToFirstSound;
    },
    get commandLog() {
      return [...commandLog];
    },
    get undoDepth() {
      return sources.project().undoDepth;
    },
    get arrangement() {
      return arrangementProbe ? arrangementProbe() : null;
    },
    resetStats: resetFrameStats,
    lastExport: null,
  };
  watchFirstSound(sources.audio);
  // With ?e2e a JSON copy lands in a data attribute so a driver can read it when eval is blocked.
  if (new URLSearchParams(location.search).has('e2e')) {
    setInterval(() => {
      document.documentElement.dataset.composerDebug = JSON.stringify(dbg);
    }, DUMP_MS);
  }
  (window as unknown as { composerDebug: ComposerDebug }).composerDebug = dbg;
  return dbg;
}
