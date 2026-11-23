// Every tunable number lives here. Each value can be overridden at build time with a
// VITE_ environment variable, so no logic file carries a hardcoded threshold.

type Env = Record<string, string | undefined>;
const env: Env = (import.meta as { env?: Env }).env ?? {};

function num(key: string, fallback: number): number {
  const raw = env[key];
  const n = raw === undefined || raw === '' ? NaN : Number(raw);
  return Number.isFinite(n) ? n : fallback;
}

function list(key: string, fallback: number[]): number[] {
  const raw = env[key];
  if (!raw) return fallback;
  const parsed = raw.split(',').map((s) => Number(s.trim()));
  return parsed.every((n) => Number.isFinite(n) && n > 0) && parsed.length > 0 ? parsed : fallback;
}

export const config = {
  /** Web plugin hosting (WebCLAP). File size cap in bytes, memory ceiling in 64 KiB pages, events per block. */
  wclapMaxFileBytes: num('VITE_WCLAP_MAX_FILE_BYTES', 16 * 1024 * 1024),
  wclapMaxMemoryPages: num('VITE_WCLAP_MAX_MEMORY_PAGES', 2048),
  wclapMaxEvents: num('VITE_WCLAP_MAX_EVENTS', 256),
  /** A block that takes more than this share of its real time length is an overrun; this many in a row bypasses the plugin. */
  wclapBudgetShare: num('VITE_WCLAP_BUDGET_SHARE', 0.8),
  wclapOverrunLimit: num('VITE_WCLAP_OVERRUN_LIMIT', 64),
  /** Plugin stats are sent to the page every this many blocks. */
  wclapLoadTimeoutMs: num('VITE_WCLAP_LOAD_TIMEOUT_MS', 15000),
  wclapReportBlocks: num('VITE_WCLAP_REPORT_BLOCKS', 32),
  /** Must match TICKS_PER_QUARTER in core/composer-model. */
  ticksPerQuarter: num('VITE_TICKS_PER_QUARTER', 960),
  /** Note value denominators offered by the snap menu (4 means a quarter note). */
  gridDenominators: list('VITE_GRID_DENOMINATORS', [4, 8, 16, 32]),
  defaultGridDenominator: num('VITE_DEFAULT_GRID', 8),
  /** Tick step used when the grid is Off (no snapping, one tick). */
  gridOffTicks: num('VITE_GRID_OFF_TICKS', 1),
  /** Deleting more notes than this at once asks first. */
  confirmDeleteNotes: num('VITE_CONFIRM_DELETE_NOTES', 8),
  /** History panel: row height in px (must equal the h-9 class), rows fetched per page and rows drawn beyond the edge. */
  historyRowPx: num('VITE_HISTORY_ROW_PX', 36),
  historyPage: num('VITE_HISTORY_PAGE', 100),
  historyOverscan: num('VITE_HISTORY_OVERSCAN', 6),
  /** Command palette: results shown at once and how many commands the local usage ranking remembers. */
  paletteMaxResults: num('VITE_PALETTE_MAX_RESULTS', 50),
  paletteUsageMax: num('VITE_PALETTE_USAGE_MAX', 200),
  /** AI assist in the palette: suggestion cards shown, how often the badge asks for waiting proposals, the longest a live preview stays open (below the daemon group timeout), commands listed before "and N more". */
  assistMaxCards: num('VITE_ASSIST_MAX_CARDS', 6),
  assistPollMs: num('VITE_ASSIST_POLL_MS', 3000),
  assistPreviewMaxMs: num('VITE_ASSIST_PREVIEW_MAX_MS', 25000),
  assistShowMax: num('VITE_ASSIST_SHOW_MAX', 40),
  /** How long a toast with an action stays up. */
  toastMs: num('VITE_TOAST_MS', 8000),
  /** Plain toasts: short text and long text (over toastLongChars) stay up this long. */
  toastShortMs: num('VITE_TOAST_SHORT_MS', 2000),
  toastLongMs: num('VITE_TOAST_LONG_MS', 5000),
  toastLongChars: num('VITE_TOAST_LONG_CHARS', 60),
  /** Most toasts on screen at once; the oldest goes first. */
  toastMax: num('VITE_TOAST_MAX', 4),
  /** Pixels per quarter note at the default zoom and its limits. */
  zoomDefault: num('VITE_ZOOM_DEFAULT', 96),
  zoomMin: num('VITE_ZOOM_MIN', 6),
  zoomMax: num('VITE_ZOOM_MAX', 640),
  zoomStep: num('VITE_ZOOM_STEP', 1.25),
  rowHeight: num('VITE_ROW_HEIGHT', 14),
  rowHeightMin: num('VITE_ROW_HEIGHT_MIN', 8),
  rowHeightMax: num('VITE_ROW_HEIGHT_MAX', 28),
  /** rulerHeight and trackRowHeight must equal the h-7 header and h-16 rows of the track list. */
  rulerHeight: num('VITE_RULER_HEIGHT', 28),
  velocityLaneHeight: num('VITE_VELOCITY_LANE', 60),
  /** The loudness lane starts folded when opening it would leave fewer pitch rows than this. */
  velocityAutoCollapseRows: num('VITE_VELOCITY_AUTO_COLLAPSE_ROWS', 14),
  keyboardWidth: num('VITE_KEYBOARD_WIDTH', 52),
  /** Width in px of the grab zone on a note's right edge. */
  noteEdgeGrab: num('VITE_NOTE_EDGE_GRAB', 6),
  /** Pointer travel in px before a press turns into a drag. */
  dragThreshold: num('VITE_DRAG_THRESHOLD', 3),
  /** Comfortable track row. Must equal the h-16 class of the track list rows. */
  trackRowHeight: num('VITE_TRACK_ROW_HEIGHT', 64),
  /** Compact track row. Must equal the h-[60px] class of the track list rows. */
  trackRowHeightCompact: num('VITE_TRACK_ROW_HEIGHT_COMPACT', 60),
  /** Height of the "Tracks" header above the lanes (the h-7 class). */
  trackHeaderHeight: num('VITE_TRACK_HEADER_HEIGHT', 28),
  /** The arrangement never shrinks below this many track lanes. */
  minArrangementRows: num('VITE_MIN_ARRANGEMENT_ROWS', 2),
  /** Smallest height in px the dock keeps when the split is dragged. */
  minDockHeight: num('VITE_MIN_DOCK_HEIGHT', 200),
  /** Arrangement share of the workspace by default (percent). */
  splitDefaultPercent: num('VITE_SPLIT_DEFAULT_PERCENT', 40),
  /** Arrangement share while the Notes tab is open and the split was never dragged (percent). */
  splitNotesPercent: num('VITE_SPLIT_NOTES_PERCENT', 30),
  /** Percent of the workspace one arrow key press moves the split. */
  splitKeyStepPercent: num('VITE_SPLIT_KEY_STEP_PERCENT', 2),
  /** Delay before a repeated announcement is spoken again. */
  announceGapMs: num('VITE_ANNOUNCE_GAP_MS', 40),
  /** Quantize strengths offered next to the Quantize button (percent); the first is the default. */
  quantizeStrengths: list('VITE_QUANTIZE_STRENGTHS', [100, 75, 50, 25]),
  /** Loudness at or below this reads Soft, at or above velocityLoudMin reads Loud, between is Medium. */
  velocitySoftMax: num('VITE_VELOCITY_SOFT_MAX', 56),
  velocityLoudMin: num('VITE_VELOCITY_LOUD_MIN', 100),
  /** Loudness change of one Alt plus arrow press. */
  velocityStep: num('VITE_VELOCITY_STEP', 8),
  /** Smallest cell in px the Auto grid may use. */
  gridMinCellPx: num('VITE_GRID_MIN_CELL_PX', 14),
  /** Note list row height in px. Must equal the h-9 class of its rows. */
  noteListRowHeight: num('VITE_NOTE_LIST_ROW', 36),
  noteListOverscan: num('VITE_NOTE_LIST_OVERSCAN', 6),
  /** Pitch names listed in the status line before it says "and N more". */
  selectionKeysMax: num('VITE_SELECTION_KEYS_MAX', 6),
  /** Storage key for settings that live only in this browser. */
  prefsKey: 'composer.prefs.v1',
  /** Width in px of the grab zone on a region's right edge (resize). */
  regionEdgeGrab: num('VITE_REGION_EDGE_GRAB', 8),
  /** Region drags snap to bars while a bar is at least this wide in px, else to beats. */
  snapBarMinPx: num('VITE_SNAP_BAR_MIN_PX', 48),
  /** Space kept on each side when the arrangement fits clips to the window. */
  fitPaddingPx: num('VITE_FIT_PADDING_PX', 24),
  /** Height in px of the horizontal scrollbar drawn at the foot of the arrangement. */
  scrollbarPx: num('VITE_SCROLLBAR_PX', 10),
  scrollbarMinThumbPx: num('VITE_SCROLLBAR_MIN_THUMB_PX', 24),
  /** Most clips the debug probe lists. */
  probeMaxClips: num('VITE_PROBE_MAX_CLIPS', 200),
  /** Mixer ranges in permille. Must match MAX_VOLUME, UNITY_VOLUME and MAX_PAN in core/composer-model. */
  volumeMax: num('VITE_VOLUME_MAX', 2000),
  volumeUnity: num('VITE_VOLUME_UNITY', 1000),
  volumeStep: num('VITE_VOLUME_STEP', 10),
  panMax: num('VITE_PAN_MAX', 1000),
  panStep: num('VITE_PAN_STEP', 20),
  /** Reverb ranges in permille. Must match MAX_REVERB in core/composer-model; the default room size matches MasterMix. */
  reverbMax: num('VITE_REVERB_MAX', 1000),
  reverbStep: num('VITE_REVERB_STEP', 10),
  reverbSizeDefault: num('VITE_REVERB_SIZE_DEFAULT', 500),
  /** Mixer fader: lowest dB shown (below it the track is silent), key step and coarse key step. */
  faderMinDb: num('VITE_FADER_MIN_DB', -60),
  faderStepDb: num('VITE_FADER_STEP_DB', 0.5),
  faderCoarseDb: num('VITE_FADER_COARSE_DB', 3),
  /** Lit segments of a track meter and the loudness (0 to 1) each full strip maps to. */
  meterSegments: num('VITE_METER_SEGMENTS', 12),
  meterFloorDb: num('VITE_METER_FLOOR_DB', -48),
  /** Mixer strip width in px. Must equal the w-24 class of a strip. */
  stripWidth: num('VITE_STRIP_WIDTH', 96),
  /** Strips rendered beyond each edge of the mixer viewport. */
  stripOverscan: num('VITE_STRIP_OVERSCAN', 2),
  /** Library sidebar: width limits and default in px, rail width, key step, row heights and audition delay. */
  libraryKey: 'composer.library.v1',
  libraryWidthDefault: num('VITE_LIBRARY_WIDTH_DEFAULT', 280),
  libraryWidthMin: num('VITE_LIBRARY_WIDTH_MIN', 220),
  libraryWidthMax: num('VITE_LIBRARY_WIDTH_MAX', 480),
  libraryRailWidth: num('VITE_LIBRARY_RAIL_WIDTH', 44),
  libraryKeyStep: num('VITE_LIBRARY_KEY_STEP', 16),
  libraryRowHeight: num('VITE_LIBRARY_ROW_HEIGHT', 64),
  libraryHeaderHeight: num('VITE_LIBRARY_HEADER_HEIGHT', 32),
  libraryOverscan: num('VITE_LIBRARY_OVERSCAN', 4),
  libraryAuditionDelayMs: num('VITE_LIBRARY_AUDITION_DELAY_MS', 250),
  /** Most devices one effect chain holds; the core refuses more (MAX_CHAIN_EFFECTS). */
  effectChainMax: num('VITE_EFFECT_CHAIN_MAX', 16),
  menuLongPressMs: num('VITE_MENU_LONG_PRESS_MS', 500),
  menuLongPressSlopPx: num('VITE_MENU_LONG_PRESS_SLOP_PX', 8),
  menuViewportMarginPx: num('VITE_MENU_VIEWPORT_MARGIN_PX', 8),
  /** Extension the daemon requires for project files. */
  projectExtension: (env.VITE_PROJECT_EXTENSION ?? '') || 'cmpf',
  arrangementZoomDefault: num('VITE_ARRANGEMENT_ZOOM', 28),
  defaultRegionBars: num('VITE_DEFAULT_REGION_BARS', 4),
  /** Most copies one Repeat x N asks for. */
  maxRepeat: num('VITE_MAX_REPEAT', 16),
  defaultNoteVelocity: num('VITE_DEFAULT_VELOCITY', 100),
  /** Wait after the last edit before the engine blob is rebuilt. */
  blobDebounceMs: num('VITE_BLOB_DEBOUNCE_MS', 120),
  /** Frames between playhead reports from the audio thread. */
  positionReportFrames: num('VITE_POSITION_REPORT_FRAMES', 1024),
  /** A reload keeps the playhead by fast forwarding up to this many seconds. */
  maxSeekSeconds: num('VITE_MAX_SEEK_SECONDS', 300),
  autosaveDebounceMs: num('VITE_AUTOSAVE_DEBOUNCE_MS', 600),
  storageKey: 'composer.project.v1',
  /** Computer keyboard piano: MIDI note of the key "a" and octave step. */
  keyboardBaseNote: num('VITE_KEYBOARD_BASE_NOTE', 60),
  keyboardOctaveMin: num('VITE_KEYBOARD_OCTAVE_MIN', -3),
  keyboardOctaveMax: num('VITE_KEYBOARD_OCTAVE_MAX', 3),
  liveVelocity: num('VITE_LIVE_VELOCITY', 100),
  /** Recording: share of the output delay taken off a press (1 places it where it was heard) and the grid it tidies to. */
  recordLatencyShare: num('VITE_RECORD_LATENCY_SHARE', 1),
  recordGridDenominator: num('VITE_RECORD_GRID', 8),
  /** Catch what I just played: how far back the buffer reaches and how many notes it keeps. */
  catchSeconds: num('VITE_CATCH_SECONDS', 30),
  catchMaxNotes: num('VITE_CATCH_MAX_NOTES', 512),
  /** On screen keys: octaves shown and chord pads offered. */
  screenKeyOctaves: num('VITE_SCREEN_KEY_OCTAVES', 2),
  chordPadMax: num('VITE_CHORD_PAD_MAX', 7),
  chordPreviewDelayMs: num('VITE_CHORD_PREVIEW_DELAY_MS', 250),
  chordPreviewMs: num('VITE_CHORD_PREVIEW_MS', 700),
  recordPollMs: num('VITE_RECORD_POLL_MS', 50),
  tempoMinBpm: num('VITE_TEMPO_MIN_BPM', 20),
  tempoMaxBpm: num('VITE_TEMPO_MAX_BPM', 400),
  /** Notes made by `?stress` when no count is given. */
  stressNotes: num('VITE_STRESS_NOTES', 20000),
  /** Commands per group while building a stress project. */
  stressChunk: num('VITE_STRESS_CHUNK', 50000),
  /** Tracks and clips `?stress` spreads its notes over when the URL gives none (bench default: 300 and 3000). */
  stressTracks: num('VITE_STRESS_TRACKS', 1),
  stressRegions: num('VITE_STRESS_REGIONS', 1),
  /** Above this many notes the store follows the change feed and stops refetching the project after each edit. */
  fullRefreshMaxNotes: num('VITE_FULL_REFRESH_MAX_NOTES', 5000),
  /** Frame service: frames closer than this are skipped (120 Hz screens). */
  minFrameMs: num('VITE_MIN_FRAME_MS', 16),
  exportSampleRate: num('VITE_EXPORT_SAMPLE_RATE', 48000),
  /** Frames per slice of the offline render before yielding to the page. */
  exportSliceFrames: num('VITE_EXPORT_SLICE_FRAMES', 48000),
  /** Bars of empty space allowed after the last region when scrolling. */
  scrollPadBars: num('VITE_SCROLL_PAD_BARS', 8),
  /** Grid lines closer than this many px are not drawn. */
  gridMinPx: num('VITE_GRID_MIN_PX', 7),
  /** Ruler labels are thinned so they sit at least this many px apart. */
  rulerLabelPx: num('VITE_RULER_LABEL_PX', 40),
  /** Region previews draw at most this many notes. */
  regionPreviewMaxNotes: num('VITE_REGION_PREVIEW_NOTES', 1500),
  /** Follow the playhead when it leaves this fraction of the roll width. */
  followEdge: num('VITE_FOLLOW_EDGE', 0.9),
  /** How often the status bar and play button re-read the audio status. */
  meterRefreshMs: num('VITE_METER_REFRESH_MS', 150),
  /** Tap tempo: a gap longer than this starts a new count, the last N taps are averaged, and the tempo is set once taps stop for the commit delay. */
  tapResetMs: num('VITE_TAP_RESET_MS', 2000),
  tapWindow: num('VITE_TAP_WINDOW', 8),
  tapMinTaps: num('VITE_TAP_MIN_TAPS', 2),
  tapCommitMs: num('VITE_TAP_COMMIT_MS', 700),
  /** Timeline lanes: row height (must equal the h-6 class), grab distance for a point and the diamond size in px. */
  laneHeight: num('VITE_LANE_HEIGHT', 24),
  laneGrabPx: num('VITE_LANE_GRAB_PX', 8),
  laneStorageKey: env.VITE_LANE_KEY ?? 'composer.lanes.v1',
  /** Clip loop lengths offered in the clip menu, in beats. */
  loopChoicesBeats: list('VITE_LOOP_CHOICES_BEATS', [1, 2, 4, 8, 16, 32]),
  /** Position readout refresh in ms. */
  positionRefreshMs: num('VITE_POSITION_REFRESH_MS', 80),
  rpcTimeoutMs: num('VITE_RPC_TIMEOUT_MS', 15000),
  /** Rolling window of roll draw times kept for the debug panel. */
  frameSamples: num('VITE_FRAME_SAMPLES', 240),
  /** Sound browser audition: root note, velocity and how long the chord rings. */
  previewRootPitch: num('VITE_PREVIEW_ROOT', 60),
  previewVelocity: num('VITE_PREVIEW_VELOCITY', 84),
  previewHoldMs: num('VITE_PREVIEW_HOLD_MS', 900),
  /** Metronome click: length, level and the two pitches (accent is the first beat of a bar). */
  clickMs: num('VITE_CLICK_MS', 28),
  clickGain: num('VITE_CLICK_GAIN', 0.32),
  clickHz: num('VITE_CLICK_HZ', 1000),
  clickAccentHz: num('VITE_CLICK_ACCENT_HZ', 1600),
  /** Frames the audio thread may spend catching a spare engine up to a seek or loop start per quantum. */
  seekBudgetFrames: num('VITE_SEEK_BUDGET_FRAMES', 2048),
  /** Count in length a fresh in-page session starts with, and the longest the picker offers (bars). */
  countInBars: num('VITE_COUNT_IN_BARS', 1),
  countInMaxBars: num('VITE_COUNT_IN_MAX_BARS', 2),
  /** Next action chip: it hides for good after this many dismissals. */
  nextActionMaxDismissals: num('VITE_NEXT_ACTION_MAX_DISMISSALS', 3),
  /** A project at least this many bars long that was never exported gets the export suggestion. */
  nextActionLongBars: num('VITE_NEXT_ACTION_LONG_BARS', 32),
  /** Recent projects listed on the start screen. */
  startRecentMax: num('VITE_START_RECENT_MAX', 6),
} as const;

export function gridTicks(denominator: number, triplet: boolean): number {
  const straight = (config.ticksPerQuarter * 4) / denominator;
  return triplet ? Math.round((straight * 2) / 3) : straight;
}

export interface GridChoice {
  id: string;
  label: string;
  ticks: number;
}

const NOTE_NAMES: Record<number, string> = { 1: 'whole', 2: 'half', 4: 'quarter', 8: 'eighth', 16: 'sixteenth', 32: 'thirty-second' };

function plainGrid(d: number, triplet: boolean): string {
  const name = NOTE_NAMES[d] ?? `1/${d}`;
  const text = triplet ? `${name} note triplets` : `${name} notes`;
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Grid menu entries: each denominator plain, then its triplet. */
export function gridChoices(): GridChoice[] {
  const out: GridChoice[] = [];
  for (const d of config.gridDenominators) {
    out.push({ id: `1/${d}`, label: plainGrid(d, false), ticks: gridTicks(d, false) });
    out.push({ id: `1/${d}t`, label: plainGrid(d, true), ticks: gridTicks(d, true) });
  }
  return out;
}
