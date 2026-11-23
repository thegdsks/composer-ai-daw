// Piano roll for the selected region, drawn on one canvas. All pointer, wheel and key
// handling lives here so React renders chrome only. Edits are sent as Backend commands.
import type { AudioEngine } from '../audio/audioEngine';
import { config } from '../config';
import { isHeld, subscribeHeld } from '../input/heldNotes';
import { newId } from '../ids';
import type { Id, Note, Region, Tick } from '../model';
import { addNote, clampVelocity, movedPosition } from '../state/commands';
import {
  copyNotesBy,
  deleteNoteIds,
  moveNotesBy,
  quantizeNotes,
  rampNotesVelocity,
  resizeNotesBy,
  setNotesVelocity,
  transposeNotes,
} from '../state/bulkNotes';
import type { Actions } from '../state/actions';
import { findRegion, sortedNotes, type Store } from '../state/store';
import { barLines, clamp, isBlackKey, meterSegments, pitchName, secondsToTicks, snapRound, ticksPerBar } from '../time';
import { CanvasHost } from '../ui/canvasHost';
import type { Palette } from '../ui/palette';
import { keyAt, keyMask, nearestInKey, stepInKey, type KeyInfo } from './keyScale';
import {
  clampGroupDelta,
  clampGroupResize,
  loudnessText,
  marqueeHits,
  noteSpoken,
  quantizedStart,
  rampVelocities,
} from './selection';
import { drawKeyRows, drawOctaveReadout, drawTooltip, positionText } from './rollOverlay';
import {
  MAX_PITCH,
  hitNote,
  hitVelocityBar,
  pitchToY,
  tickToX,
  velocityFromY,
  visibleRange,
  xToTick,
  yToPitch,
  zoomAround,
  type Viewport,
} from './geometry';

const VELOCITY_BAR_PX = 4;
const NOTE_GAP_PX = 1;
/** A note drawn this recently by the first click of a double click is not deleted by the second. */
const DOUBLE_CLICK_GUARD_MS = 900;
const clampPitchTo = (p: number): number => Math.min(MAX_PITCH, Math.max(0, p));
type CursorClass = 'cursor-ew-resize' | 'cursor-grab' | 'cursor-grabbing' | 'cursor-pointer' | 'cursor-crosshair';

/** Tool choices the toolbar owns; read through a getter so no rebuild is needed. */
export interface RollMode {
  /** Select makes a drag on empty grid a marquee (no Shift needed). */
  tool: 'draw' | 'select';
  /** A drag in the velocity lane draws a ramp over the selection instead of setting one value. */
  ramp: boolean;
  /** Quantize strength in percent. */
  strength: number;
}

const DEFAULT_MODE: RollMode = { tool: 'draw', ramp: false, strength: 100 };

interface RampLine {
  t0: Tick;
  v0: number;
  t1: Tick;
  v1: number;
}

type Drag =
  | { kind: 'draw'; pitch: number; start: Tick; end: Tick; dragged: boolean }
  | { kind: 'marquee'; tickA: Tick; tickB: Tick; pitchA: number; pitchB: number; base: Id[]; hits: Id[] }
  | {
      kind: 'move';
      grab: Note;
      ids: Id[];
      originTick: number;
      originPitch: number;
      dTick: Tick;
      dPitch: number;
      moved: boolean;
      copy: boolean;
    }
  | { kind: 'resize'; grab: Note; ids: Id[]; dDur: Tick; moved: boolean }
  | { kind: 'velocity'; ids: Id[]; value: number; ramp: RampLine | null; byRange: boolean; preview: Map<Id, number> }
  | { kind: 'key'; pitch: number };

export interface RollView {
  scrollTick: number;
  minTick: number;
  maxTick: number;
  pxPerTick: number;
  scrollY: number;
  maxScrollY: number;
  rowHeight: number;
  /** The loudness lane is showing (auto may have folded it). */
  laneOpen: boolean;
  /** Whole pitch rows the grid shows right now. */
  rows: number;
}

export interface RollDeps {
  canvas: HTMLCanvasElement;
  store: Store;
  audio: AudioEngine;
  actions: Actions;
  getPalette(): Palette | null;
  /** Current snap step in ticks. */
  getGrid(): number;
  preview(pitch: number, on: boolean): void;
  onView(): void;
  /** Stay in key switch; notes drawn or moved land on the nearest key pitch while it is on. */
  getStayInKey?(): boolean;
  /** Tool, ramp and quantize strength; defaults to draw, set and 100 percent. */
  getMode?(): RollMode;
  /** Loudness lane choice; auto folds it when it would leave too few pitch rows. */
  getLane?(): 'auto' | 'open' | 'closed';
}

export class RollController {
  private readonly host: CanvasHost;
  private vp: Viewport;
  private drag: Drag | null = null;
  private committing = false;
  private laneOpen = true;
  private reportedLane = true;
  private regionKey: Id | null = null;
  private pointerId: number | null = null;
  private pressedKey = -1;
  private lastPlayTick = -1;
  private cursor = '';
  private ptr = { x: 0, y: 0 };
  private keyCache: { key: KeyInfo | null; mask: boolean[] } = { key: null, mask: [] };
  private readonly unsubs: (() => void)[] = [];
  private readonly maxDurCache = new WeakMap<Region, number>();
  /** Loudness and length the next drawn note starts with: what the user used last. */
  private lastVelocity = config.defaultNoteVelocity;
  private lastLength = 0;
  private justDrawn: { id: Id; at: number } | null = null;
  private quantizeGhost = false;
  private hoverVelocity: Id | null = null;

  constructor(private readonly d: RollDeps) {
    this.vp = {
      scrollTick: 0,
      scrollY: 0,
      pxPerTick: config.zoomDefault / config.ticksPerQuarter,
      rowHeight: config.rowHeight,
      width: 1,
      height: 1,
    };
    this.host = new CanvasHost({
      canvas: d.canvas,
      draw: (ctx, w, h) => this.draw(ctx, w, h),
      animating: () => this.playheadMoved(),
      measure: true,
    });
    const c = d.canvas;
    c.addEventListener('pointerdown', this.onDown);
    c.addEventListener('pointermove', this.onMove);
    c.addEventListener('pointerup', this.onUp);
    c.addEventListener('pointercancel', this.onCancel);
    c.addEventListener('dblclick', this.onDouble);
    c.addEventListener('wheel', this.onWheel, { passive: false });
    c.addEventListener('keydown', this.onKey);
    this.unsubs.push(d.store.subscribe(this.onStore), d.audio.subscribe(() => this.host.invalidate()), subscribeHeld(() => this.host.invalidate()));
    this.onStore();
  }

  destroy(): void {
    const c = this.d.canvas;
    c.removeEventListener('pointerdown', this.onDown);
    c.removeEventListener('pointermove', this.onMove);
    c.removeEventListener('pointerup', this.onUp);
    c.removeEventListener('pointercancel', this.onCancel);
    c.removeEventListener('dblclick', this.onDouble);
    c.removeEventListener('wheel', this.onWheel);
    c.removeEventListener('keydown', this.onKey);
    for (const u of this.unsubs) u();
    this.host.destroy();
  }

  invalidate(): void {
    this.host.invalidate();
  }

  // ---- geometry -----------------------------------------------------------

  private layout(): { gx: number; gy: number; gw: number; gh: number; velTop: number } {
    const gx = config.keyboardWidth;
    const gy = config.rulerHeight;
    const gw = Math.max(1, this.host.width - gx);
    const want = this.d.getLane?.() ?? 'open';
    const freeWithLane = (this.host.height - gy - config.velocityLaneHeight) / Math.max(1, this.vp.rowHeight);
    this.laneOpen = want === 'open' || (want === 'auto' && freeWithLane >= config.velocityAutoCollapseRows);
    const velTop = this.laneOpen ? Math.max(gy + 1, this.host.height - config.velocityLaneHeight) : this.host.height;
    const gh = Math.max(1, velTop - gy);
    this.vp.width = gw;
    this.vp.height = gh;
    return { gx, gy, gw, gh, velTop };
  }

  private current(): { region: Region; notes: Note[]; maxDur: number } | null {
    const snap = this.d.store.getSnapshot();
    const found = findRegion(snap.project, snap.selectedRegion);
    if (!found) return null;
    const notes = sortedNotes(found.region);
    let maxDur = this.maxDurCache.get(found.region);
    if (maxDur === undefined) {
      maxDur = 0;
      for (const n of notes) if (n.duration > maxDur) maxDur = n.duration;
      this.maxDurCache.set(found.region, maxDur);
    }
    return { region: found.region, notes, maxDur };
  }

  private clampView(): void {
    const snap = this.d.store.getSnapshot();
    const bar = config.ticksPerQuarter * 4;
    let end = 0;
    for (const t of snap.project.tracks) for (const r of Object.values(t.regions)) end = Math.max(end, r.start + r.length);
    const max = end + bar * config.scrollPadBars;
    const span = this.vp.width / this.vp.pxPerTick;
    this.vp.scrollTick = clamp(this.vp.scrollTick, -bar, Math.max(-bar, max - span));
    const maxY = Math.max(0, (MAX_PITCH + 1) * this.vp.rowHeight - this.vp.height);
    this.vp.scrollY = clamp(this.vp.scrollY, 0, maxY);
  }

  getView(): RollView {
    const snap = this.d.store.getSnapshot();
    const bar = config.ticksPerQuarter * 4;
    let end = 0;
    for (const t of snap.project.tracks) for (const r of Object.values(t.regions)) end = Math.max(end, r.start + r.length);
    const span = this.vp.width / this.vp.pxPerTick;
    return {
      scrollTick: this.vp.scrollTick,
      minTick: -bar,
      maxTick: Math.max(0, end + bar * config.scrollPadBars - span),
      pxPerTick: this.vp.pxPerTick,
      scrollY: this.vp.scrollY,
      maxScrollY: Math.max(0, (MAX_PITCH + 1) * this.vp.rowHeight - this.vp.height),
      rowHeight: this.vp.rowHeight,
      laneOpen: this.laneOpen,
      rows: Math.floor(this.vp.height / this.vp.rowHeight),
    };
  }

  setScrollY(y: number): void {
    this.vp.scrollY = y;
    this.afterView();
  }

  /** Key in force at the open clip and its pitch lookup; null when the project has no key. */
  private keyState(): { key: KeyInfo; mask: boolean[] } | null {
    const snap = this.d.store.getSnapshot();
    const found = findRegion(snap.project, snap.selectedRegion);
    const key = keyAt(snap.project.tempo_map, found?.region.start ?? 0);
    if (!key) return null;
    const c = this.keyCache;
    if (!c.key || c.key.root !== key.root || c.key.scale !== key.scale) this.keyCache = { key, mask: keyMask(key) };
    return { key, mask: this.keyCache.mask };
  }

  /** Pitch after Stay in key; unchanged when it is off or the project has no key. */
  private keep(pitch: number): number {
    if (!this.d.getStayInKey?.()) return pitch;
    const ks = this.keyState();
    return ks ? nearestInKey(ks.mask, pitch) : pitch;
  }

  setScrollTick(tick: number): void {
    this.vp.scrollTick = tick;
    this.afterView();
  }

  private afterView(): void {
    this.layout();
    this.clampView();
    this.host.invalidate();
    this.d.onView();
  }

  zoomBy(factor: number, anchorX?: number): void {
    this.layout();
    const min = config.zoomMin / config.ticksPerQuarter;
    const max = config.zoomMax / config.ticksPerQuarter;
    const next = clamp(this.vp.pxPerTick * factor, min, max);
    this.vp = zoomAround(this.vp, anchorX ?? this.vp.width / 2, next);
    this.afterView();
  }

  /** Frames the selected region and centers its pitches. */
  fit(): void {
    const cur = this.current();
    this.layout();
    if (!cur) return;
    const def = config.zoomDefault / config.ticksPerQuarter;
    const fitPx = (this.vp.width * 0.94) / cur.region.length;
    this.vp.pxPerTick = clamp(fitPx, def * 0.5, def * 2);
    this.vp.scrollTick = cur.region.start - (this.vp.width * 0.03) / this.vp.pxPerTick;
    let lo = MAX_PITCH;
    let hi = 0;
    for (const n of cur.notes) {
      lo = Math.min(lo, n.pitch);
      hi = Math.max(hi, n.pitch);
    }
    const mid = cur.notes.length ? (lo + hi) / 2 : 60;
    this.vp.scrollY = (MAX_PITCH - mid) * this.vp.rowHeight - this.vp.height / 2;
    this.afterView();
  }

  private onStore = (): void => {
    const snap = this.d.store.getSnapshot();
    if (snap.selectedRegion !== this.regionKey) {
      this.regionKey = snap.selectedRegion;
      this.fit();
    }
    this.host.invalidate();
  };

  // ---- playhead -----------------------------------------------------------

  private playTick(): number {
    const s = this.d.audio.status;
    if (s.sampleRate <= 0) return 0;
    const map = this.d.store.getSnapshot().project.tempo_map;
    return secondsToTicks(map, s.positionFrames / s.sampleRate);
  }

  private playheadMoved(): boolean {
    const t = this.playTick();
    if (t === this.lastPlayTick) return false;
    return true;
  }

  // ---- drawing ------------------------------------------------------------

  private draw(ctx: CanvasRenderingContext2D, w: number, h: number): void {
    const pal = this.d.getPalette();
    if (!pal) return;
    const { gx, gy, gw, gh, velTop } = this.layout();
    const cur = this.current();
    const playTick = this.playTick();
    this.lastPlayTick = playTick;
    this.followPlayhead(playTick);
    const vp = this.vp;
    const map = this.d.store.getSnapshot().project.tempo_map;
    const storeSel = this.d.store.getSnapshot().selectedNotes;
    const selected = this.drag?.kind === 'marquee' ? new Set([...this.drag.base, ...this.drag.hits]) : storeSel;
    const grid = this.d.getGrid();

    ctx.fillStyle = pal.bg;
    ctx.fillRect(0, 0, w, h);

    // ---- grid area (clipped, translated)
    ctx.save();
    ctx.beginPath();
    ctx.rect(gx, gy, gw, gh);
    ctx.clip();
    ctx.translate(gx, gy);

    const topPitch = yToPitch(vp, 0);
    const botPitch = yToPitch(vp, gh);
    ctx.fillStyle = pal.row;
    for (let p = botPitch; p <= topPitch; p++) {
      if (isBlackKey(p)) ctx.fillRect(0, pitchToY(vp, p), gw, vp.rowHeight);
    }
    const ks = this.keyState();
    if (ks) drawKeyRows(ctx, pal, vp, ks.mask, ks.key, gw, gh);
    ctx.fillStyle = pal.line;
    for (let p = botPitch; p <= topPitch; p++) ctx.fillRect(0, pitchToY(vp, p), gw, 1);

    if (cur) {
      const rx0 = tickToX(vp, cur.region.start);
      const rx1 = tickToX(vp, cur.region.start + cur.region.length);
      ctx.fillStyle = pal.outside;
      ctx.globalAlpha = 0.7;
      if (rx0 > 0) ctx.fillRect(0, 0, Math.min(gw, rx0), gh);
      if (rx1 < gw) ctx.fillRect(Math.max(0, rx1), 0, gw - Math.max(0, rx1), gh);
      ctx.globalAlpha = 1;
    }

    const vis = { from: vp.scrollTick, to: xToTick(vp, gw) };
    const gridPx = grid * vp.pxPerTick;
    if (gridPx >= config.gridMinPx) {
      ctx.fillStyle = pal.line;
      const first = Math.floor(vis.from / grid) * grid;
      for (let t = first; t < vis.to; t += grid) ctx.fillRect(Math.round(tickToX(vp, t)), 0, 1, gh);
    }
    const lines = barLines(map, vis.from, vis.to, config.ticksPerQuarter * vp.pxPerTick >= config.gridMinPx * 2);
    for (const l of lines) {
      ctx.fillStyle = l.isBar ? pal.bar : pal.beat;
      ctx.fillRect(Math.round(tickToX(vp, l.tick)), 0, 1, gh);
    }

    if (cur) this.drawNotes(ctx, cur, pal, selected, gw, gh, vis);
    this.drawMarquee(ctx, pal);
    ctx.restore();
    if (cur) this.drawDragTip(ctx, cur.region, pal, w, h);

    // ---- velocity lane
    if (this.laneOpen !== this.reportedLane) {
      this.reportedLane = this.laneOpen;
      this.d.onView();
    }
    ctx.save();
    ctx.beginPath();
    ctx.rect(gx, velTop, gw, h - velTop);
    ctx.clip();
    ctx.translate(gx, velTop);
    const laneH = h - velTop;
    ctx.fillStyle = pal.row;
    ctx.fillRect(0, 0, gw, laneH);
    ctx.fillStyle = pal.bar;
    ctx.fillRect(0, 0, gw, 1);
    for (const l of lines) {
      if (l.isBar) {
        ctx.fillStyle = pal.line;
        ctx.fillRect(Math.round(tickToX(vp, l.tick)), 0, 1, laneH);
      }
    }
    if (cur) this.drawVelocity(ctx, cur, pal, selected, gw, laneH);
    ctx.restore();
    if (cur) this.drawVelocityTip(ctx, cur, pal, w, h, velTop);

    // ---- keyboard column
    this.drawKeys(ctx, pal, gy, gh, h, velTop);

    // ---- ruler
    ctx.fillStyle = pal.keyWhite;
    ctx.fillRect(0, 0, w, gy);
    ctx.fillStyle = pal.bar;
    ctx.fillRect(0, gy - 1, w, 1);
    drawOctaveReadout(ctx, pal, vp, gh, gx, gy);
    ctx.save();
    ctx.beginPath();
    ctx.rect(gx, 0, gw, gy);
    ctx.clip();
    ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
    ctx.textBaseline = 'middle';
    const barPx = ticksPerBar(meterSegments(map)[0]?.sig ?? { numerator: 4, denominator: 4 }) * vp.pxPerTick;
    const every = Math.max(1, Math.ceil(config.rulerLabelPx / Math.max(1, barPx)));
    for (const l of lines) {
      if (!l.isBar) continue;
      const x = gx + Math.round(tickToX(vp, l.tick));
      ctx.fillStyle = pal.bar;
      ctx.fillRect(x, gy - 8, 1, 8);
      if (l.bar % every === 0) {
        ctx.fillStyle = pal.inkDim;
        ctx.fillText(String(l.bar + 1), x + 4, gy / 2 - 1);
      }
    }
    ctx.restore();

    // ---- playhead across ruler, grid and lane
    const px = gx + tickToX(vp, playTick);
    if (px >= gx && px <= w) {
      ctx.fillStyle = pal.playhead;
      ctx.fillRect(Math.round(px), 0, 2, h);
    }
    // Read by e2e scripts to aim the pointer at a pitch row.
    this.d.canvas.dataset.view = `${vp.scrollY},${vp.rowHeight},${vp.scrollTick},${vp.pxPerTick}`;
    // Layout probe for the e2e layout script: whole pitch rows and whether the loudness lane shows.
    this.d.canvas.dataset.rows = String(Math.floor(vp.height / vp.rowHeight));
    this.d.canvas.dataset.lane = this.laneOpen ? 'open' : 'closed';
  }

  private drawNotes(
    ctx: CanvasRenderingContext2D,
    cur: { region: Region; notes: Note[]; maxDur: number },
    pal: Palette,
    selected: ReadonlySet<Id>,
    gw: number,
    gh: number,
    vis: { from: number; to: number },
  ): void {
    const vp = this.vp;
    const { region, notes, maxDur } = cur;
    const drag = this.drag;
    const [lo, hi] = visibleRange(notes, maxDur, vis.from - region.start, vis.to - region.start);
    // A dragged note can be far from its stored place, so those are drawn from the selection.
    const copying = drag?.kind === 'move' && drag.copy;
    const live = drag && (drag.kind === 'move' || drag.kind === 'resize') && !copying ? new Set(drag.ids) : null;
    const draw = (n: Note, start: Tick, pitch: number, duration: Tick, sel: boolean, ghost: boolean): void => {
      const x = tickToX(vp, region.start + start);
      const wpx = Math.max(2, duration * vp.pxPerTick - NOTE_GAP_PX);
      if (x + wpx < 0 || x > gw) return;
      const y = pitchToY(vp, pitch);
      if (y + vp.rowHeight < 0 || y > gh) return;
      ctx.globalAlpha = ghost ? 0.55 : 0.5 + (0.5 * n.velocity) / 127;
      ctx.fillStyle = sel ? pal.noteSelected : pal.note;
      ctx.fillRect(x, y + 1, wpx, vp.rowHeight - 1);
      if (wpx > 4) {
        ctx.globalAlpha = 1;
        ctx.fillStyle = sel ? pal.noteSelected : pal.noteEdge;
        ctx.fillRect(x, y + 1, 1, vp.rowHeight - 1);
        ctx.fillRect(x, y + 1, wpx, 1);
      }
    };
    for (let i = lo; i < hi; i++) {
      const n = notes[i];
      if (!n || live?.has(n.id)) continue;
      draw(n, n.start, n.pitch, n.duration, selected.has(n.id), false);
    }
    if (drag && (drag.kind === 'move' || drag.kind === 'resize')) {
      const ghosts = new Set(drag.ids);
      for (const n of notes) {
        if (!ghosts.has(n.id)) continue;
        if (drag.kind === 'move') {
          const m = movedPosition(region, n, drag.dTick, drag.dPitch);
          draw(n, m.start, m.pitch, n.duration, true, true);
        } else {
          const dur = Math.max(1, Math.min(n.duration + drag.dDur, region.length - n.start));
          draw(n, n.start, n.pitch, dur, true, true);
        }
      }
    }
    if (drag?.kind === 'draw') {
      const ghost: Note = {
        id: 'draw',
        pitch: drag.pitch,
        velocity: config.defaultNoteVelocity,
        start: drag.start,
        duration: Math.max(1, drag.end - drag.start),
      };
      draw(ghost, ghost.start, ghost.pitch, ghost.duration, true, true);
    }
    if (this.quantizeGhost) this.drawQuantizeGhost(ctx, cur, draw, lo, hi);
    ctx.globalAlpha = 1;
  }

  /** Where Quantize would put the notes it moves, drawn as ghosts while its button is hovered or focused. */
  private drawQuantizeGhost(
    ctx: CanvasRenderingContext2D,
    cur: { region: Region; notes: Note[]; maxDur: number },
    draw: (n: Note, start: Tick, pitch: number, duration: Tick, sel: boolean, ghost: boolean) => void,
    lo: number,
    hi: number,
  ): void {
    const grid = this.d.getGrid();
    const strength = this.mode().strength;
    const sel = this.d.store.getSnapshot().selectedNotes;
    for (let i = lo; i < hi; i++) {
      const n = cur.notes[i];
      if (!n || (sel.size > 0 && !sel.has(n.id))) continue;
      const s = quantizedStart(n, grid, strength, cur.region.length);
      if (s !== n.start) draw(n, s, n.pitch, n.duration, true, true);
    }
  }

  private drawMarquee(ctx: CanvasRenderingContext2D, pal: Palette): void {
    const d = this.drag;
    if (d?.kind !== 'marquee') return;
    const vp = this.vp;
    const x0 = tickToX(vp, Math.min(d.tickA, d.tickB));
    const x1 = tickToX(vp, Math.max(d.tickA, d.tickB));
    const y0 = pitchToY(vp, Math.max(d.pitchA, d.pitchB));
    const y1 = pitchToY(vp, Math.min(d.pitchA, d.pitchB)) + vp.rowHeight;
    ctx.globalAlpha = 0.18;
    ctx.fillStyle = pal.accent;
    ctx.fillRect(x0, y0, x1 - x0, y1 - y0);
    ctx.globalAlpha = 1;
    ctx.strokeStyle = pal.accent;
    ctx.setLineDash([4, 3]);
    ctx.strokeRect(x0 + 0.5, y0 + 0.5, x1 - x0, y1 - y0);
    ctx.setLineDash([]);
  }

  /** Pitch and bar of the note being dragged, next to the pointer. */
  private drawDragTip(ctx: CanvasRenderingContext2D, region: Region, pal: Palette, w: number, h: number): void {
    const d = this.drag;
    if (!d || (d.kind !== 'draw' && d.kind !== 'move' && d.kind !== 'resize')) return;
    const map = this.d.store.getSnapshot().project.tempo_map;
    const bar = ticksPerBar(meterSegments(map)[0]?.sig ?? { numerator: 4, denominator: 4 });
    let pitch: number;
    let start: Tick;
    if (d.kind === 'draw') {
      pitch = d.pitch;
      start = d.start;
    } else if (d.kind === 'move') {
      const m = movedPosition(region, d.grab, d.dTick, d.dPitch);
      pitch = m.pitch;
      start = m.start;
    } else {
      pitch = d.grab.pitch;
      start = d.grab.start;
    }
    drawTooltip(ctx, pal, `${pitchName(pitch)}  ${positionText(region.start + start, bar)}`, this.ptr.x, this.ptr.y, w, h);
  }

  private drawVelocity(
    ctx: CanvasRenderingContext2D,
    cur: { region: Region; notes: Note[]; maxDur: number },
    pal: Palette,
    selected: ReadonlySet<Id>,
    gw: number,
    laneH: number,
  ): void {
    const vp = this.vp;
    const { region, notes } = cur;
    const [lo, hi] = visibleRange(notes, 0, vp.scrollTick - region.start, xToTick(vp, gw) - region.start);
    const drag = this.drag;
    const vel = drag?.kind === 'velocity' ? drag : null;
    for (let i = lo; i < hi; i++) {
      const n = notes[i];
      if (!n) continue;
      const x = tickToX(vp, region.start + n.start);
      if (x < -VELOCITY_BAR_PX || x > gw) continue;
      const v = vel?.preview.get(n.id) ?? n.velocity;
      const bh = Math.max(2, (v / 127) * (laneH - 4));
      ctx.fillStyle = selected.has(n.id) ? pal.noteSelected : pal.note;
      ctx.fillRect(x, laneH - bh, VELOCITY_BAR_PX, bh);
    }
    if (vel?.ramp) {
      const r = vel.ramp;
      const y = (v: number): number => laneH - Math.max(2, (v / 127) * (laneH - 4));
      ctx.strokeStyle = pal.accent;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(tickToX(vp, r.t0), y(r.v0));
      ctx.lineTo(tickToX(vp, r.t1), y(r.v1));
      ctx.stroke();
      ctx.lineWidth = 1;
    }
  }

  /** Soft, Medium or Loud with the number, next to the pointer while a velocity drag runs or a bar is hovered. */
  private drawVelocityTip(
    ctx: CanvasRenderingContext2D,
    cur: { region: Region; notes: Note[] },
    pal: Palette,
    w: number,
    h: number,
    velTop: number,
  ): void {
    const d = this.drag;
    let text = '';
    if (d?.kind === 'velocity') text = loudnessText(d.ramp ? d.ramp.v1 : d.value);
    else if (!d && this.hoverVelocity) {
      const n = cur.notes.find((x) => x.id === this.hoverVelocity);
      if (n) text = loudnessText(n.velocity);
    }
    if (!text) return;
    drawTooltip(ctx, pal, text, this.ptr.x, Math.max(velTop + 16, this.ptr.y - 8), w, h);
  }

  private drawKeys(
    ctx: CanvasRenderingContext2D,
    pal: Palette,
    gy: number,
    gh: number,
    h: number,
    velTop: number,
  ): void {
    const vp = this.vp;
    const gx = config.keyboardWidth;
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, gy, gx, gh);
    ctx.clip();
    ctx.translate(0, gy);
    ctx.font = '10px ui-sans-serif, system-ui, sans-serif';
    ctx.textBaseline = 'middle';
    const top = yToPitch(vp, 0);
    const bot = yToPitch(vp, gh);
    const ks = this.keyState();
    for (let p = bot; p <= top; p++) {
      const y = pitchToY(vp, p);
      const black = isBlackKey(p);
      ctx.fillStyle = p === this.pressedKey || isHeld(p) ? pal.noteSelected : black ? pal.keyBlack : pal.keyWhite;
      ctx.fillRect(0, y, black ? gx * 0.62 : gx, vp.rowHeight);
      ctx.fillStyle = pal.line;
      ctx.fillRect(0, y, gx, 1);
      if (p % 12 === 0 || p % 12 === ks?.key.root) {
        ctx.fillStyle = p % 12 === 0 ? pal.ink : pal.accent;
        ctx.fillText(pitchName(p), gx - 26, y + vp.rowHeight / 2);
      }
    }
    ctx.restore();
    ctx.fillStyle = pal.keyWhite;
    ctx.fillRect(0, velTop, gx, h - velTop);
    if (velTop < h) {
      ctx.fillStyle = pal.keyLabel;
      ctx.font = '10px ui-sans-serif, system-ui, sans-serif';
      ctx.textBaseline = 'middle';
      ctx.fillText('Velocity', 4, velTop + 12);
    }
    ctx.fillStyle = pal.bar;
    ctx.fillRect(gx - 1, gy, 1, h - gy);
  }

  private followPlayhead(tick: number): void {
    if (this.d.audio.status.transport !== 'playing' || this.drag || !this.d.store.getSnapshot().transport.follow) return;
    const x = tickToX(this.vp, tick);
    if (x > this.vp.width * config.followEdge || x < 0) {
      this.vp.scrollTick = tick - (this.vp.width * 0.1) / this.vp.pxPerTick;
      this.clampView();
      this.d.onView();
    }
  }

  // ---- pointer ------------------------------------------------------------

  private local(e: { clientX: number; clientY: number }): { lx: number; ly: number } {
    const r = this.d.canvas.getBoundingClientRect();
    return { lx: e.clientX - r.left, ly: e.clientY - r.top };
  }

  private mode(): RollMode {
    return this.d.getMode?.() ?? DEFAULT_MODE;
  }

  private setCursor(name: CursorClass | ''): void {
    if (name === this.cursor) return;
    if (this.cursor) this.d.canvas.classList.remove(this.cursor);
    if (name) this.d.canvas.classList.add(name);
    this.cursor = name;
  }

  private onDown = (e: PointerEvent): void => {
    if (e.button !== 0) return;
    const { lx, ly } = this.local(e);
    const { gx, gy, gh, velTop } = this.layout();
    this.d.canvas.focus({ preventScroll: true });
    const cur = this.current();
    const gxp = lx - gx;
    const gyp = ly - gy;
    if (ly < gy) return;
    if (lx < gx && ly < velTop) {
      const pitch = yToPitch(this.vp, gyp);
      this.drag = { kind: 'key', pitch };
      this.pressedKey = pitch;
      this.d.preview(pitch, true);
      this.capture(e);
      this.host.invalidate();
      return;
    }
    if (!cur) return;
    const sel = new Set(this.d.store.getSnapshot().selectedNotes);
    if (ly >= velTop) {
      const hit = hitVelocityBar(cur.notes, cur.region.start, this.vp, gxp, VELOCITY_BAR_PX);
      const value = velocityFromY(ly - velTop, config.velocityLaneHeight);
      if (e.altKey || this.mode().ramp) {
        // A ramp covers the selection, or every note between the two ends of the stroke.
        const t = xToTick(this.vp, gxp);
        const ids = sel.size > 0 ? [...sel] : [];
        this.drag = { kind: 'velocity', ids, value, ramp: { t0: t, v0: value, t1: t, v1: value }, byRange: sel.size === 0, preview: new Map() };
        this.updateVelocity(gxp, ly - velTop);
        this.capture(e);
        return;
      }
      // With a selection a press anywhere in the lane sets all of it; without one it needs a bar.
      if (!hit && sel.size === 0) return;
      if (hit && !sel.has(hit.id)) {
        sel.clear();
        sel.add(hit.id);
        this.d.store.selectNotes(sel);
      }
      this.drag = { kind: 'velocity', ids: [...sel], value, ramp: null, byRange: false, preview: new Map() };
      this.updateVelocity(gxp, ly - velTop);
      this.capture(e);
      return;
    }
    if (lx < gx || ly >= gy + gh) return;
    const [lo, hi] = visibleRange(
      cur.notes,
      cur.maxDur,
      xToTick(this.vp, 0) - cur.region.start,
      xToTick(this.vp, this.vp.width) - cur.region.start,
    );
    const hit = hitNote(cur.notes.slice(lo, hi), cur.region.start, this.vp, gxp, gyp, config.noteEdgeGrab);
    if (hit) {
      if (e.shiftKey) {
        if (sel.has(hit.note.id)) sel.delete(hit.note.id);
        else sel.add(hit.note.id);
        this.d.store.selectNotes(sel);
        if (!sel.has(hit.note.id)) return;
      } else if (!sel.has(hit.note.id)) {
        sel.clear();
        sel.add(hit.note.id);
        this.d.store.selectNotes(sel);
      }
      const ids = [...sel];
      if (hit.zone === 'edge') this.drag = { kind: 'resize', grab: hit.note, ids, dDur: 0, moved: false };
      else {
        this.drag = {
          kind: 'move',
          grab: hit.note,
          ids,
          originTick: xToTick(this.vp, gxp),
          originPitch: yToPitch(this.vp, gyp),
          dTick: 0,
          dPitch: 0,
          moved: false,
          copy: false,
        };
        this.d.preview(hit.note.pitch, true);
        this.pressedKey = hit.note.pitch;
      }
      this.capture(e);
      return;
    }
    // Empty space: a marquee with Shift or the Select tool (it may start outside the clip), else draw a note.
    this.ptr = { x: lx, y: ly };
    if (e.shiftKey || this.mode().tool === 'select') {
      const base = e.shiftKey ? [...sel] : [];
      if (!e.shiftKey) this.d.store.selectNotes([]);
      const t = xToTick(this.vp, gxp);
      const p = yToPitch(this.vp, gyp);
      this.drag = { kind: 'marquee', tickA: t, tickB: t, pitchA: p, pitchB: p, base, hits: [] };
      this.capture(e);
      this.host.invalidate();
      return;
    }
    const grid = this.d.getGrid();
    const local = xToTick(this.vp, gxp) - cur.region.start;
    if (local < 0 || local >= cur.region.length) {
      this.d.store.selectNotes([]);
      return;
    }
    const start = Math.floor(local / grid) * grid;
    this.d.store.selectNotes([]);
    const length = Math.min(Math.max(grid, this.lastLength), cur.region.length - start);
    this.drag = { kind: 'draw', pitch: this.keep(yToPitch(this.vp, gyp)), start, end: start + Math.max(1, length), dragged: false };
    this.d.preview(this.drag.pitch, true);
    this.pressedKey = this.drag.pitch;
    this.capture(e);
    this.host.invalidate();
  };

  private capture(e: PointerEvent): void {
    this.pointerId = e.pointerId;
    this.d.canvas.setPointerCapture(e.pointerId);
  }

  /** Follows the pointer in the lane: one value for the notes, or a ramp line with a live preview. */
  private updateVelocity(gx: number, laneY: number): void {
    const d = this.drag;
    const cur = this.current();
    if (d?.kind !== 'velocity' || !cur) return;
    const v = velocityFromY(laneY, config.velocityLaneHeight);
    d.value = v;
    d.preview = new Map();
    if (d.ramp) {
      d.ramp.t1 = xToTick(this.vp, gx);
      d.ramp.v1 = v;
      const a = Math.min(d.ramp.t0, d.ramp.t1) - cur.region.start;
      const b = Math.max(d.ramp.t0, d.ramp.t1) - cur.region.start;
      if (d.byRange) d.ids = cur.notes.filter((n) => n.start >= a && n.start <= b).map((n) => n.id);
      const byId = new Map(cur.notes.map((n) => [n.id, n]));
      const picked = d.ids.flatMap((id) => byId.get(id) ?? []);
      const [from, to] = d.ramp.t0 <= d.ramp.t1 ? [d.ramp.v0, d.ramp.v1] : [d.ramp.v1, d.ramp.v0];
      d.preview = rampVelocities(picked, from, to);
    } else {
      for (const id of d.ids) d.preview.set(id, v);
    }
    this.host.invalidate();
  }

  private onMove = (e: PointerEvent): void => {
    const { lx, ly } = this.local(e);
    const { gx, gy, gh, velTop } = this.layout();
    const gxp = lx - gx;
    const gyp = ly - gy;
    this.ptr = { x: lx, y: ly };
    const drag = this.drag;
    const cur = this.current();
    if (!drag) {
      this.hover(lx, ly, gxp, gyp, gh, velTop, cur);
      return;
    }
    if (this.committing) return;
    const grid = this.d.getGrid();
    switch (drag.kind) {
      case 'key':
        break;
      case 'velocity':
        this.updateVelocity(gxp, ly - velTop);
        break;
      case 'marquee': {
        if (!cur) break;
        drag.tickB = xToTick(this.vp, gxp);
        drag.pitchB = yToPitch(this.vp, gyp);
        drag.hits = marqueeHits(cur.notes, cur.maxDur, cur.region.start, drag);
        this.host.invalidate();
        break;
      }
      case 'draw': {
        if (!cur) break;
        const local = xToTick(this.vp, gxp) - cur.region.start;
        if (!drag.dragged && Math.abs(local - drag.start) * this.vp.pxPerTick >= config.dragThreshold) drag.dragged = true;
        if (drag.dragged) {
          const end = Math.max(drag.start + grid, Math.ceil(local / grid) * grid);
          drag.end = Math.min(end, cur.region.length);
        }
        const pitch = this.keep(yToPitch(this.vp, gyp));
        if (pitch !== drag.pitch) {
          this.d.preview(drag.pitch, false);
          drag.pitch = pitch;
          this.d.preview(pitch, true);
          this.pressedKey = pitch;
        }
        this.host.invalidate();
        break;
      }
      case 'move': {
        const dx = xToTick(this.vp, gxp) - drag.originTick;
        const dp = yToPitch(this.vp, gyp) - drag.originPitch;
        if (!drag.moved && Math.abs(dx) * this.vp.pxPerTick < config.dragThreshold && dp === 0) break;
        drag.moved = true;
        // Snap the grabbed note's start, then move every selected note by the same step, as a block.
        if (!cur) break;
        const ids = new Set(drag.ids);
        const group = cur.notes.filter((n) => ids.has(n.id));
        const d = clampGroupDelta(
          cur.region,
          group,
          snapRound(drag.grab.start + dx, grid) - drag.grab.start,
          this.keep(drag.grab.pitch + dp) - drag.grab.pitch,
        );
        drag.dTick = d.dTick;
        drag.dPitch = d.dPitch;
        drag.copy = e.altKey;
        this.setCursor('cursor-grabbing');
        this.host.invalidate();
        break;
      }
      case 'resize': {
        if (!cur) break;
        const end = xToTick(this.vp, gxp) - cur.region.start;
        const target = Math.max(grid, snapRound(end - drag.grab.start, grid));
        if (!drag.moved && Math.abs(target - drag.grab.duration) * this.vp.pxPerTick < config.dragThreshold) break;
        drag.moved = true;
        const ids = new Set(drag.ids);
        const group = cur.notes.filter((n) => ids.has(n.id));
        drag.dDur = clampGroupResize(cur.region, group, target - drag.grab.duration, grid);
        this.host.invalidate();
        break;
      }
    }
  };

  private hover(
    lx: number,
    ly: number,
    gxp: number,
    gyp: number,
    gh: number,
    velTop: number,
    cur: { region: Region; notes: Note[]; maxDur: number } | null,
  ): void {
    if (!cur || ly < config.rulerHeight) return this.setCursor('');
    if (lx < config.keyboardWidth) return this.setCursor('cursor-pointer');
    if (ly >= velTop) {
      const bar = hitVelocityBar(cur.notes, cur.region.start, this.vp, gxp, VELOCITY_BAR_PX);
      const id = bar?.id ?? null;
      if (id !== this.hoverVelocity) {
        this.hoverVelocity = id;
        this.host.invalidate();
      }
      return this.setCursor('cursor-pointer');
    }
    if (this.hoverVelocity) {
      this.hoverVelocity = null;
      this.host.invalidate();
    }
    if (ly >= config.rulerHeight + gh) return this.setCursor('');
    const hit = hitNote(cur.notes, cur.region.start, this.vp, gxp, gyp, config.noteEdgeGrab);
    this.setCursor(hit ? (hit.zone === 'edge' ? 'cursor-ew-resize' : 'cursor-grab') : 'cursor-crosshair');
  }

  private onUp = (e: PointerEvent): void => {
    const drag = this.drag;
    if (!drag) return;
    if (drag.kind === 'move') drag.copy = e.altKey;
    if (this.pointerId !== null && this.d.canvas.hasPointerCapture(this.pointerId)) {
      this.d.canvas.releasePointerCapture(this.pointerId);
    }
    this.pointerId = null;
    if (this.pressedKey >= 0) this.d.preview(this.pressedKey, false);
    this.pressedKey = -1;
    this.setCursor('');
    void this.commit(drag);
  };

  private onCancel = (): void => {
    if (this.pressedKey >= 0) this.d.preview(this.pressedKey, false);
    this.pressedKey = -1;
    this.drag = null;
    this.pointerId = null;
    this.host.invalidate();
  };

  /** Sends the drag as commands; the preview stays on screen until the new state arrives. */
  private async commit(drag: Drag): Promise<void> {
    const store = this.d.store;
    const cur = this.current();
    this.committing = true;
    try {
      if (!cur || drag.kind === 'key') return;
      const byId = new Map(cur.notes.map((n) => [n.id, n]));
      const picked = (ids: Id[]): Note[] => ids.flatMap((id) => byId.get(id) ?? []);
      switch (drag.kind) {
        case 'draw': {
          const dur = Math.max(1, Math.min(drag.end, cur.region.length) - drag.start);
          const id = newId();
          const r = await store.apply(addNote(id, cur.region.id, drag.pitch, drag.start, dur, this.lastVelocity));
          if (r.ok) {
            store.selectNotes([id]);
            this.justDrawn = { id, at: performance.now() };
            if (drag.dragged) this.lastLength = dur;
          }
          break;
        }
        case 'marquee':
          store.selectNotes([...new Set([...drag.base, ...drag.hits])]);
          break;
        case 'move':
          if (!drag.moved) break;
          if (drag.copy) {
            const label = `Copy ${drag.ids.length === 1 ? 'note' : `${drag.ids.length} notes`}`;
            const r = await store.group(label, copyNotesBy(cur.region, picked(drag.ids), drag.dTick, drag.dPitch, newId));
            if (r.ok) this.d.store.selectNotes(drag.ids);
          } else {
            await store.group(this.moveLabel(cur, drag.ids, drag.dTick, drag.dPitch), moveNotesBy(cur.region.id, drag.ids, drag.dTick, drag.dPitch));
          }
          break;
        case 'resize':
          if (drag.moved) {
            await store.group(drag.ids.length === 1 ? 'Resize note' : `Resize ${drag.ids.length} notes`, resizeNotesBy(cur.region.id, drag.ids, drag.dDur));
          }
          break;
        case 'velocity': {
          const label = drag.ids.length === 1 ? 'Set loudness' : `Set loudness of ${drag.ids.length} notes`;
          if (drag.ramp) {
            const r = drag.ramp;
            const [from, to] = r.t0 <= r.t1 ? [r.v0, r.v1] : [r.v1, r.v0];
            await store.group(`Ramp loudness of ${drag.ids.length} notes`, rampNotesVelocity(cur.region.id, drag.ids, from, to));
          } else {
            const ids = picked(drag.ids).filter((n) => n.velocity !== drag.value).map((n) => n.id);
            this.lastVelocity = clampVelocity(drag.value);
            await store.group(label, setNotesVelocity(cur.region.id, ids, drag.value));
          }
          break;
        }
      }
    } finally {
      this.committing = false;
      this.drag = null;
      this.host.invalidate();
    }
  }

  // ---- wheel and keys -----------------------------------------------------

  private onWheel = (e: WheelEvent): void => {
    e.preventDefault();
    const { lx } = this.local(e);
    const { gx } = this.layout();
    if (e.ctrlKey || e.metaKey) {
      this.zoomBy(Math.exp(-e.deltaY * 0.01), lx - gx);
    } else if (e.altKey) {
      this.vp.rowHeight = clamp(this.vp.rowHeight * Math.exp(-e.deltaY * 0.01), config.rowHeightMin, config.rowHeightMax);
      this.afterView();
    } else if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      const dx = e.shiftKey && e.deltaX === 0 ? e.deltaY : e.deltaX;
      this.vp.scrollTick += dx / this.vp.pxPerTick;
      this.afterView();
    } else {
      this.vp.scrollY += e.deltaY;
      this.afterView();
    }
  };

  /** Deletes the selected notes as one undo step. */
  async deleteSelection(): Promise<void> {
    const cur = this.current();
    const sel = this.d.store.getSnapshot().selectedNotes;
    if (!cur || sel.size === 0) return;
    const ids = cur.notes.filter((n) => sel.has(n.id)).map((n) => n.id);
    await this.d.store.group(ids.length === 1 ? 'Delete note' : `Delete ${ids.length} notes`, deleteNoteIds(cur.region.id, ids));
  }

  /** Plain step label for a move: names the landing spot of a single note, else the count. */
  private moveLabel(cur: { region: Region; notes: Note[] }, ids: readonly Id[], dTick: Tick, dPitch: number): string {
    const only = ids.length === 1 ? cur.notes.find((n) => n.id === ids[0]) : undefined;
    if (!only) return `Move ${ids.length} notes`;
    const map = this.d.store.getSnapshot().project.tempo_map;
    const bar = ticksPerBar(meterSegments(map)[0]?.sig ?? { numerator: 4, denominator: 4 });
    return `Moved to ${noteSpoken({ ...only, start: only.start + dTick, pitch: clampPitchTo(only.pitch + dPitch) }, cur.region.start, bar)}`;
  }

  /** Q: quantize the selection, or every note when none is selected. Returns false when the grid is Off. */
  async quantizeSelection(): Promise<boolean> {
    const cur = this.current();
    const grid = this.d.getGrid();
    if (!cur || cur.notes.length === 0) return false;
    if (grid <= config.gridOffTicks) {
      this.d.store.say('Pick a grid size first, then tidy the timing');
      return false;
    }
    const sel = this.d.store.getSnapshot().selectedNotes;
    const ids = sel.size > 0 ? cur.notes.filter((n) => sel.has(n.id)).map((n) => n.id) : null;
    const count = ids ? ids.length : cur.notes.length;
    const strength = this.mode().strength;
    const r = await this.d.store.group(
      `Tidy timing of ${count} ${count === 1 ? 'note' : 'notes'}${strength < 100 ? ` at ${strength} percent` : ''}`,
      [quantizeNotes(cur.region.id, grid, strength, ids)],
    );
    return r.ok;
  }

  /** Shows where Quantize would put notes while its button is hovered or focused. */
  setQuantizeGhost(on: boolean): void {
    if (this.quantizeGhost === on) return;
    this.quantizeGhost = on;
    this.host.invalidate();
  }

  /** Shifts the selection (every note when none) by semitones in one command. */
  async transposeSelection(semitones: number): Promise<void> {
    const cur = this.current();
    if (!cur || cur.notes.length === 0) return;
    const sel = this.d.store.getSnapshot().selectedNotes;
    const picked = sel.size > 0 ? cur.notes.filter((n) => sel.has(n.id)) : cur.notes;
    const lo = Math.min(...picked.map((n) => n.pitch));
    const hi = Math.max(...picked.map((n) => n.pitch));
    const shift = Math.min(Math.max(semitones, -lo), 127 - hi);
    if (shift === 0) return;
    const ids = sel.size > 0 ? picked.map((n) => n.id) : null;
    await this.d.store.group(`Transpose ${picked.length === 1 ? 'note' : `${picked.length} notes`} ${shift > 0 ? 'up' : 'down'}`, [
      transposeNotes(cur.region.id, shift, ids),
    ]);
  }

  /** Alt plus up or down: each selected note gets louder or softer by one step, keeping their differences. */
  async nudgeLoudness(dir: 1 | -1): Promise<void> {
    const cur = this.current();
    const sel = this.d.store.getSnapshot().selectedNotes;
    if (!cur || sel.size === 0) return;
    const groups = new Map<number, Id[]>();
    for (const n of cur.notes) {
      if (!sel.has(n.id)) continue;
      const v = clampVelocity(n.velocity + dir * config.velocityStep);
      if (v !== n.velocity) groups.set(v, [...(groups.get(v) ?? []), n.id]);
    }
    const cmds = [...groups].flatMap(([v, ids]) => setNotesVelocity(cur.region.id, ids, v));
    if (cmds.length === 0) return;
    await this.d.store.group(dir > 0 ? 'Louder' : 'Softer', cmds);
  }

  /** Double click on a note that was there before the click removes it. */
  private onDouble = (e: MouseEvent): void => {
    const cur = this.current();
    if (!cur) return;
    const { lx, ly } = this.local(e);
    const { gx, gy, gh } = this.layout();
    if (lx < gx || ly < gy || ly >= gy + gh) return;
    const hit = hitNote(cur.notes, cur.region.start, this.vp, lx - gx, ly - gy, config.noteEdgeGrab);
    if (!hit) return;
    // The first click of a double click on empty grid just drew this note; keep it.
    const fresh = this.justDrawn;
    if (fresh && fresh.id === hit.note.id && performance.now() - fresh.at < DOUBLE_CLICK_GUARD_MS) return;
    void this.d.store.group('Delete note', deleteNoteIds(cur.region.id, [hit.note.id]));
  };

  selectAll(): void {
    const cur = this.current();
    if (cur) this.d.store.selectNotes(cur.notes.map((n) => n.id));
  }

  /** Ctrl or Cmd C, X, V, D and Shift D (Repeat x N) act on notes through Actions. */
  private clipboardKey(e: KeyboardEvent, region: Id, sel: ReadonlySet<Id>, grid: number): void {
    const a = this.d.actions;
    const keep = (p: number): number => this.keep(p);
    const key = e.key.toLowerCase();
    if (key === 'c' && !e.shiftKey) a.copyNotes(region, sel);
    else if (key === 'x' && !e.shiftKey) void a.cutNotes(region, sel);
    else if (key === 'v' && !e.shiftKey) void a.pasteNotes(region, this.playTick(), keep);
    else if (key === 'd' && e.shiftKey) void a.repeatNotes(region, sel, grid, keep);
    else if (key === 'd') void a.duplicateNotes(region, sel, grid, 1, keep);
    else return;
    e.preventDefault();
  }

  private onKey = (e: KeyboardEvent): void => {
    const cur = this.current();
    if (!cur) return;
    const mod = e.ctrlKey || e.metaKey;
    const grid = this.d.getGrid();
    const sel = this.d.store.getSnapshot().selectedNotes;
    if (mod && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      this.selectAll();
      return;
    }
    if (mod) {
      this.clipboardKey(e, cur.region.id, sel, grid);
      return;
    }
    if (e.key === 'Escape') {
      this.d.store.selectNotes([]);
      return;
    }
    if (e.key.toLowerCase() === 'q' && !e.altKey) {
      e.preventDefault();
      void this.quantizeSelection();
      return;
    }
    if (e.key === 'Enter') {
      // Keyboard way to add a note: one grid step after the selection, or at the region start.
      e.preventDefault();
      const last = cur.notes.filter((n) => sel.has(n.id)).sort((a, b) => b.start + b.duration - (a.start + a.duration))[0];
      const start = last ? last.start + last.duration : 0;
      if (start + grid <= cur.region.length) {
        const id = newId();
        void this.d.store.apply(addNote(id, cur.region.id, last?.pitch ?? config.keyboardBaseNote, start, grid)).then((r) => {
          if (r.ok) this.d.store.selectNotes([id]);
        });
      }
      return;
    }
    if (e.key === 'Home') {
      this.fit();
      return;
    }
    if (e.key === '+' || e.key === '=') return this.zoomBy(config.zoomStep);
    if (e.key === '-') return this.zoomBy(1 / config.zoomStep);
    const arrows = ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'];
    if (!arrows.includes(e.key)) return;
    e.preventDefault();
    if (sel.size === 0) {
      const first = cur.notes[0];
      if (first) {
        this.d.store.selectNotes([first.id]);
        this.d.store.say(noteSpoken(first, cur.region.start, this.barTicks()));
      }
      return;
    }
    const notes = cur.notes.filter((n) => sel.has(n.id));
    const ids = notes.map((n) => n.id);
    const dir = e.key === 'ArrowRight' || e.key === 'ArrowUp' ? 1 : -1;
    const horizontal = e.key === 'ArrowLeft' || e.key === 'ArrowRight';
    if (e.altKey && !horizontal) {
      void this.nudgeLoudness(dir);
      return;
    }
    if (horizontal && e.shiftKey) {
      const delta = clampGroupResize(cur.region, notes, dir * grid, Math.min(grid, ...notes.map((n) => n.duration)));
      const label = notes.length === 1 ? 'Resize note' : `Resize ${notes.length} notes`;
      void this.d.store.group(label, resizeNotesBy(cur.region.id, ids, delta));
      return;
    }
    let dTick = 0;
    let dPitch = 0;
    if (horizontal) dTick = dir * grid;
    else {
      const ks = this.d.getStayInKey?.() ? this.keyState() : null;
      const first = notes[0];
      // With Stay in key an arrow steps to the next key pitch; octaves keep their pitch class.
      dPitch = ks && first && !e.shiftKey ? stepInKey(ks.mask, first.pitch, dir) - first.pitch : dir * (e.shiftKey ? 12 : 1);
    }
    const d = clampGroupDelta(cur.region, notes, dTick, dPitch);
    void this.d.store.group(this.moveLabel(cur, ids, d.dTick, d.dPitch), moveNotesBy(cur.region.id, ids, d.dTick, d.dPitch));
  };

  private barTicks(): Tick {
    const map = this.d.store.getSnapshot().project.tempo_map;
    return ticksPerBar(meterSegments(map)[0]?.sig ?? { numerator: 4, denominator: 4 });
  }
}
