// Arrangement strip: one lane per track with its clips, on one canvas. Only the lanes in
// view are drawn. Vertical scroll is owned by the track list next to it. Gestures preview
// here and commit once on release through Actions, so one gesture is one undo step.
import type { AudioEngine } from '../audio/audioEngine';
import { config } from '../config';
import { recordArrangementFrame, setArrangementProbe, type ArrangementProbe } from '../debug';
import { trackRowHeight, subscribePrefs } from '../prefs';
import { notesOf, type Id, type Region, type Track } from '../model';
import type { Actions, ClipPlacement } from '../state/actions';
import type { TransportActions } from '../state/transportActions';
import { RulerGesture } from './rulerGesture';
import { isMenuKey } from '../ui/menuModel';
import { laneTarget, type DropTarget } from '../ui/dnd';
import { projectEnd, type Store } from '../state/store';
import { barLines, clamp, meterSegments, secondsToTicks, snapFloor, snapRound, ticksPerBar } from '../time';
import { CanvasHost } from '../ui/canvasHost';
import type { Palette } from '../ui/palette';
import {
  barsText,
  clipBars,
  clipNearTick,
  dropLane,
  fitView,
  hitClip,
  laneAt,
  neighbourClip,
  scrollFromThumb,
  scrollThumb,
  type LaneView,
} from './geometry';
import { movedStart, resizedLength, snapStep } from './regionDrag';
import { clampTrim, trimStartCommands } from './trim';

export interface ArrangementDeps {
  canvas: HTMLCanvasElement;
  store: Store;
  audio: AudioEngine;
  actions: Actions;
  transport: TransportActions;
  getPalette(): Palette | null;
  /** Asks the track list to scroll; it reports back through setScrollY. */
  scrollListBy(dy: number): void;
  /** Enter on a clip: show it in the Notes editor. */
  openClip(regionId: Id): void;
  /** Right click or Shift+F10 on a clip: the panel shows the menu at the screen point. */
  onMenu?(target: ClipMenuTarget, at: { x: number; y: number }): void;
}

export interface ClipMenuTarget {
  trackId: Id;
  regionId: Id;
  /** The clips the menu acts on (the selection, which holds the clicked clip). */
  ids: readonly Id[];
}

interface Origin {
  start: number;
  length: number;
  lane: number;
}

interface ClipDrag {
  mode: 'move' | 'resize' | 'trim';
  anchorId: Id;
  /** Clips that travel together (the selection for a move, only the anchor for a resize). */
  origins: Map<Id, Origin>;
  originX: number;
  originY: number;
  downTick: number;
  pointerId: number;
  active: boolean;
  /** Preview, drawn until release. */
  dStart: number;
  dLane: number;
  nextLength: number;
  copy: boolean;
}

interface ScrollDrag {
  pointerId: number;
  grab: number;
}

/** Spelled out in full so Tailwind sees each class. */
type Cursor = 'cursor-pointer' | 'cursor-ew-resize' | 'cursor-grab' | 'cursor-grabbing' | 'cursor-copy' | 'cursor-default';

const MIN_PX_PER_TICK = config.zoomMin / config.ticksPerQuarter / 4;
const MAX_PX_PER_TICK = config.zoomMax / config.ticksPerQuarter;

export class ArrangementController {
  private readonly host: CanvasHost;
  private drag: ClipDrag | null = null;
  private scrollDrag: ScrollDrag | null = null;
  private cursor: Cursor = 'cursor-pointer';
  /** Alt as seen from key events, so a drag follows it even when pointer events do not carry it. */
  private altHeld = false;
  private scrollTick = 0;
  private scrollY = 0;
  private pxPerTick = config.arrangementZoomDefault / config.ticksPerQuarter;
  private lastPlayTick = -1;
  private readonly ruler: RulerGesture;
  private readonly unsubs: (() => void)[] = [];

  constructor(private readonly d: ArrangementDeps) {
    this.ruler = new RulerGesture({
      canvas: d.canvas,
      view: () => ({ scrollTick: this.scrollTick, pxPerTick: this.pxPerTick }),
      loop: () => d.store.getSnapshot().transport.loop,
      step: (free) => snapStep(this.sig(), this.pxPerTick, free),
      seek: (tick) => void d.transport.seek(tick),
      setLoop: (start, end) => void d.transport.setLoop(start, end),
      invalidate: () => this.host.invalidate(),
    });
    this.host = new CanvasHost({
      canvas: d.canvas,
      draw: (ctx, w, h) => this.draw(ctx, w, h),
      animating: () => this.playTick() !== this.lastPlayTick,
      onDraw: recordArrangementFrame,
    });
    d.canvas.addEventListener('pointerdown', this.onDown);
    d.canvas.addEventListener('dblclick', this.onDouble);
    d.canvas.addEventListener('contextmenu', this.onContext);
    d.canvas.addEventListener('pointermove', this.onMove);
    d.canvas.addEventListener('pointerup', this.onUp);
    d.canvas.addEventListener('pointercancel', this.onCancel);
    d.canvas.addEventListener('keydown', this.onKey);
    d.canvas.addEventListener('keyup', this.onKeyUp);
    d.canvas.addEventListener('blur', this.onBlur);
    d.canvas.addEventListener('wheel', this.onWheel, { passive: false });
    this.unsubs.push(d.store.subscribe(() => this.host.invalidate()), d.audio.subscribe(() => this.host.invalidate()), subscribePrefs(() => this.host.invalidate()));
    setArrangementProbe(() => this.probe());
  }

  destroy(): void {
    const c = this.d.canvas;
    c.removeEventListener('pointerdown', this.onDown);
    c.removeEventListener('dblclick', this.onDouble);
    c.removeEventListener('contextmenu', this.onContext);
    c.removeEventListener('pointermove', this.onMove);
    c.removeEventListener('pointerup', this.onUp);
    c.removeEventListener('pointercancel', this.onCancel);
    c.removeEventListener('keydown', this.onKey);
    c.removeEventListener('keyup', this.onKeyUp);
    c.removeEventListener('blur', this.onBlur);
    c.removeEventListener('wheel', this.onWheel);
    for (const u of this.unsubs) u();
    setArrangementProbe(null);
    this.host.destroy();
  }

  /** The visible window, read by the timeline lanes so they follow scroll and zoom. */
  getView(): { scrollTick: number; pxPerTick: number } {
    return { scrollTick: this.scrollTick, pxPerTick: this.pxPerTick };
  }

  setScrollY(y: number): void {
    this.scrollY = y;
    this.host.invalidate();
  }

  // ---- view: zoom, fit, scroll (view state, never a command) ----

  zoomBy(factor: number, anchorPx = this.host.width / 2): void {
    const anchor = anchorPx / this.pxPerTick + this.scrollTick;
    this.pxPerTick = clamp(this.pxPerTick * factor, MIN_PX_PER_TICK, MAX_PX_PER_TICK);
    this.scrollTick = anchor - anchorPx / this.pxPerTick;
    this.host.invalidate();
  }

  /** Fits the selected clips, or the whole song when `selection` is false or nothing is selected. */
  fit(selection: boolean): void {
    const snap = this.d.store.getSnapshot();
    let from = Infinity;
    let to = 0;
    for (const t of snap.project.tracks) {
      for (const r of Object.values(t.regions)) {
        if (selection && !snap.selectedRegions.has(r.id)) continue;
        from = Math.min(from, r.start);
        to = Math.max(to, r.start + r.length);
      }
    }
    if (!Number.isFinite(from)) {
      if (selection) return this.fit(false);
      this.pxPerTick = config.arrangementZoomDefault / config.ticksPerQuarter;
      this.scrollTick = 0;
    } else {
      const v = fitView(from, to, this.host.width, config.fitPaddingPx, MIN_PX_PER_TICK, MAX_PX_PER_TICK);
      this.pxPerTick = v.pxPerTick;
      this.scrollTick = v.scrollTick;
    }
    this.host.invalidate();
    this.d.store.say(selection ? 'Fitted to the selection' : 'Fitted to the whole song');
  }

  home(): void {
    this.scrollTick = 0;
    void this.d.transport.seek(0);
    this.host.invalidate();
    this.d.store.say('Start of the song');
  }

  /** Cuts the selected clips at the playhead. */
  async splitAtPlayhead(): Promise<void> {
    const ids = this.selectedIds();
    if (ids.length === 0) return;
    await this.d.actions.splitClips(ids, Math.round(this.playTick()));
  }

  private playTick(): number {
    const s = this.d.audio.status;
    if (s.sampleRate <= 0) return 0;
    return secondsToTicks(this.d.store.getSnapshot().project.tempo_map, s.positionFrames / s.sampleRate);
  }

  private totalTicks(): number {
    const bar = config.ticksPerQuarter * 4;
    return projectEnd(this.d.store.getSnapshot().project) + bar * config.scrollPadBars;
  }

  private clampScroll(): void {
    const span = this.host.width / this.pxPerTick;
    this.scrollTick = clamp(this.scrollTick, 0, Math.max(0, this.totalTicks() - span));
  }

  private laneView(): LaneView {
    return {
      top: config.rulerHeight,
      rowHeight: trackRowHeight(),
      scrollY: this.scrollY,
      laneCount: this.d.store.getSnapshot().project.tracks.length,
    };
  }

  // ---- drawing ----

  private draw(ctx: CanvasRenderingContext2D, w: number, h: number): void {
    const pal = this.d.getPalette();
    if (!pal) return;
    const snap = this.d.store.getSnapshot();
    const map = snap.project.tempo_map;
    const rh = trackRowHeight();
    const top = config.rulerHeight;
    const playTick = this.playTick();
    this.lastPlayTick = playTick;
    this.followPlayhead(playTick, w);
    this.clampScroll();
    const from = this.scrollTick;
    const to = from + w / this.pxPerTick;
    const x = (t: number): number => (t - from) * this.pxPerTick;
    const drag = this.drag?.active ? this.drag : null;

    ctx.fillStyle = pal.bg;
    ctx.fillRect(0, 0, w, h);

    const laneCount = snap.project.tracks.length;
    const first = Math.max(0, Math.floor(this.scrollY / rh));
    const last = Math.min(laneCount - 1, Math.ceil((this.scrollY + h - top) / rh));
    const lines = barLines(map, from, to, false);

    ctx.save();
    ctx.beginPath();
    ctx.rect(0, top, w, h - top);
    ctx.clip();
    for (let i = first; i <= last; i++) {
      const track = snap.project.tracks[i];
      if (!track) continue;
      const y = top + i * rh - this.scrollY;
      ctx.fillStyle = track.id === snap.selectedTrack ? pal.row : pal.bg;
      ctx.fillRect(0, y, w, rh);
      ctx.fillStyle = pal.line;
      ctx.fillRect(0, y + rh - 1, w, 1);
    }
    ctx.fillStyle = pal.line;
    for (const l of lines) ctx.fillRect(Math.round(x(l.tick)), top, 1, h - top);
    const brace = this.ruler.preview ?? snap.transport.loop;
    const braceOn = this.ruler.preview !== null || snap.transport.loop.on;
    if (brace.end > brace.start) {
      ctx.globalAlpha = braceOn ? 0.12 : 0.05;
      ctx.fillStyle = pal.loop;
      ctx.fillRect(x(brace.start), top, (brace.end - brace.start) * this.pxPerTick, h - top);
      ctx.globalAlpha = 1;
    }
    for (let i = first; i <= last; i++) {
      const track = snap.project.tracks[i];
      if (!track) continue;
      const y = top + i * rh - this.scrollY;
      for (const r of Object.values(track.regions)) {
        if (r.start + r.length < from || r.start > to) continue;
        const moving = drag?.mode === 'move' && drag.origins.has(r.id);
        const resizing = drag?.mode === 'resize' && drag.anchorId === r.id;
        const trimming = drag?.mode === 'trim' && drag.anchorId === r.id;
        const len = resizing ? drag.nextLength : trimming ? r.length - drag.dStart : r.length;
        const shift = trimming ? drag.dStart : 0;
        ctx.globalAlpha = moving ? 0.35 : 1;
        this.drawRegion(ctx, pal, r, r.start, x(r.start + shift), y + 3, Math.max(2, len * this.pxPerTick), rh - 7, snap.selectedRegions.has(r.id), from, to);
      }
    }
    ctx.globalAlpha = 1;
    if (drag?.mode === 'move') this.drawGhosts(ctx, pal, drag, laneCount, x, rh, top);
    ctx.restore();

    // Ruler
    ctx.fillStyle = pal.keyWhite;
    ctx.fillRect(0, 0, w, top);
    ctx.fillStyle = pal.bar;
    ctx.fillRect(0, top - 1, w, 1);
    ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
    ctx.textBaseline = 'middle';
    const barPx = ticksPerBar(meterSegments(map)[0]?.sig ?? { numerator: 4, denominator: 4 }) * this.pxPerTick;
    const every = Math.max(1, Math.ceil(config.rulerLabelPx / Math.max(1, barPx)));
    for (const l of lines) {
      const px = Math.round(x(l.tick));
      ctx.fillStyle = pal.bar;
      ctx.fillRect(px, top - 8, 1, 8);
      if (l.bar % every === 0) {
        ctx.fillStyle = pal.inkDim;
        ctx.fillText(String(l.bar + 1), px + 4, top / 2 - 1);
      }
    }
    if (brace.end > brace.start) this.drawBrace(ctx, pal, x(brace.start), x(brace.end), top, braceOn);
    const ph = x(playTick);
    if (ph >= 0 && ph <= w) {
      ctx.fillStyle = pal.playhead;
      ctx.fillRect(Math.round(ph), 0, 2, h);
    }
    this.drawScrollbar(ctx, pal, w, h);
  }

  /** The loop brace on the ruler: a band with a handle at each end; faint while the loop is off. */
  private drawBrace(ctx: CanvasRenderingContext2D, pal: Palette, x0: number, x1: number, top: number, on: boolean): void {
    ctx.globalAlpha = on ? 0.55 : 0.25;
    ctx.fillStyle = pal.loop;
    ctx.fillRect(x0, 0, x1 - x0, top - 1);
    ctx.globalAlpha = on ? 1 : 0.5;
    ctx.fillRect(x0, 0, 3, top - 1);
    ctx.fillRect(x1 - 3, 0, 3, top - 1);
    ctx.globalAlpha = 1;
  }

  /** With Follow on, a playing playhead that nears the right edge or leaves the left pages the view. */
  private followPlayhead(playTick: number, w: number): void {
    if (!this.d.store.getSnapshot().transport.follow || this.d.audio.status.transport !== 'playing') return;
    if (this.drag || this.scrollDrag || this.ruler.active) return;
    const span = w / this.pxPerTick;
    if (playTick > this.scrollTick + span * config.followEdge || playTick < this.scrollTick) {
      this.scrollTick = Math.max(0, playTick - span * (1 - config.followEdge));
    }
  }

  private drawGhosts(
    ctx: CanvasRenderingContext2D,
    pal: Palette,
    drag: ClipDrag,
    laneCount: number,
    x: (t: number) => number,
    rh: number,
    top: number,
  ): void {
    ctx.setLineDash([4, 3]);
    ctx.lineWidth = 1;
    let newTrack = false;
    for (const [id, o] of drag.origins) {
      const lane = o.lane + drag.dLane;
      const y = top + lane * rh - this.scrollY;
      if (lane >= laneCount) newTrack = true;
      const start = o.start + drag.dStart;
      ctx.globalAlpha = 0.35;
      ctx.fillStyle = pal.accent;
      ctx.fillRect(x(start), y + 3, o.length * this.pxPerTick, rh - 7);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = pal.accent;
      ctx.strokeRect(x(start) + 0.5, y + 3.5, o.length * this.pxPerTick - 1, rh - 8);
      if (id === drag.anchorId && drag.copy) {
        ctx.fillStyle = pal.ink;
        ctx.fillText('Copy', x(start) + 6, y + rh / 2);
      }
    }
    if (newTrack) {
      const y = top + laneCount * rh - this.scrollY;
      ctx.strokeStyle = pal.inkDim;
      ctx.strokeRect(0.5, y + 0.5, this.host.width - 1, rh - 1);
      ctx.fillStyle = pal.inkDim;
      ctx.fillText('Drop here to make a new track', 8, y + 10);
    }
    ctx.setLineDash([]);
  }

  private drawScrollbar(ctx: CanvasRenderingContext2D, pal: Palette, w: number, h: number): void {
    const span = w / this.pxPerTick;
    const thumb = scrollThumb(this.scrollTick, span, this.totalTicks(), w, config.scrollbarMinThumbPx);
    if (!thumb) return;
    const y = h - config.scrollbarPx;
    ctx.globalAlpha = 0.18;
    ctx.fillStyle = pal.inkDim;
    ctx.fillRect(0, y, w, config.scrollbarPx);
    ctx.globalAlpha = this.scrollDrag ? 0.9 : 0.6;
    ctx.fillRect(thumb.x, y + 1, thumb.width, config.scrollbarPx - 2);
    ctx.globalAlpha = 1;
  }

  private drawRegion(
    ctx: CanvasRenderingContext2D,
    pal: Palette,
    r: Region,
    startTick: number,
    x: number,
    y: number,
    w: number,
    h: number,
    selected: boolean,
    from: number,
    to: number,
  ): void {
    const alpha = ctx.globalAlpha;
    ctx.globalAlpha = alpha * 0.28;
    ctx.fillStyle = pal.note;
    ctx.fillRect(x, y, w, h);
    ctx.globalAlpha = alpha;
    ctx.fillStyle = selected ? pal.noteSelected : pal.noteEdge;
    ctx.fillRect(x, y, w, selected ? 2 : 1);
    ctx.fillRect(x, y + h - 1, w, selected ? 2 : 1);
    ctx.fillRect(x, y, 1, h);
    ctx.fillRect(x + w - 1, y, 1, h);
    // Miniature of the notes, thinned so a huge region stays cheap.
    const notes = notesOf(r);
    if (notes.length === 0) return;
    const stride = Math.max(1, Math.ceil(notes.length / config.regionPreviewMaxNotes));
    let lo = 127;
    let hi = 0;
    for (let i = 0; i < notes.length; i += stride) {
      const p = notes[i]?.pitch ?? 0;
      lo = Math.min(lo, p);
      hi = Math.max(hi, p);
    }
    const range = Math.max(1, hi - lo);
    ctx.fillStyle = selected ? pal.noteSelected : pal.noteEdge;
    for (let i = 0; i < notes.length; i += stride) {
      const n = notes[i];
      if (!n) continue;
      const abs = startTick + n.start;
      if (abs + n.duration < from || abs > to) continue;
      const nx = x + n.start * this.pxPerTick;
      const ny = y + 3 + (1 - (n.pitch - lo) / range) * (h - 8);
      ctx.fillRect(nx, ny, Math.max(1, n.duration * this.pxPerTick), 2);
    }
  }

  // ---- hit testing ----

  private point(e: { clientX: number; clientY: number }): { px: number; py: number } {
    const rect = this.d.canvas.getBoundingClientRect();
    return { px: e.clientX - rect.left, py: e.clientY - rect.top };
  }

  private hit(e: { clientX: number; clientY: number }): {
    lane: number;
    below: boolean;
    track: Track | null;
    clip: { region: Region; zone: 'body' | 'edge' | 'trim' } | null;
    tick: number;
  } | null {
    const { px, py } = this.point(e);
    const at = laneAt(this.laneView(), py);
    if (!at) return null;
    const track = this.d.store.getSnapshot().project.tracks[at.lane] ?? null;
    const tick = px / this.pxPerTick + this.scrollTick;
    const found = track ? hitClip(Object.values(track.regions), this.scrollTick, this.pxPerTick, px, config.regionEdgeGrab, config.regionEdgeGrab) : null;
    const region = found && track ? track.regions[found.clip.id] : undefined;
    // Audio clips have no notes to keep in place, so only note clips offer the left edge.
    if (found?.zone === 'trim' && region && !('midi' in region.content)) found.zone = 'body';
    return { lane: at.lane, below: at.below, track, clip: found && region ? { region, zone: found.zone } : null, tick };
  }

  private sig(): { numerator: number; denominator: number } {
    return meterSegments(this.d.store.getSnapshot().project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 };
  }

  private barTicks(): number {
    return ticksPerBar(this.sig());
  }

  private laneOf(id: Id): number {
    return this.d.store.getSnapshot().project.tracks.findIndex((t) => t.regions[id] !== undefined);
  }

  private selectedIds(): Id[] {
    const snap = this.d.store.getSnapshot();
    const ids = [...snap.selectedRegions];
    if (ids.length === 0 && snap.selectedRegion) ids.push(snap.selectedRegion);
    return ids;
  }

  private setCursor(next: Cursor): void {
    if (next === this.cursor) return;
    this.d.canvas.classList.remove(this.cursor);
    this.d.canvas.classList.add(next);
    this.cursor = next;
  }

  private inScrollbar(py: number): boolean {
    if (py < this.host.height - config.scrollbarPx) return false;
    return scrollThumb(this.scrollTick, this.host.width / this.pxPerTick, this.totalTicks(), this.host.width, config.scrollbarMinThumbPx) !== null;
  }

  // ---- pointer ----

  private onDown = (e: PointerEvent): void => {
    this.d.canvas.focus({ preventScroll: true });
    const { px, py } = this.point(e);
    if (this.ruler.down(e, px, py)) return;
    if (e.button === 0 && this.inScrollbar(py)) return this.startScrollDrag(e, px);
    const h = this.hit(e);
    if (!h) return;
    const store = this.d.store;
    if (!h.track) return;
    if (!h.clip) {
      store.selectTrack(h.track.id);
      return;
    }
    const region = h.clip.region;
    const snap = store.getSnapshot();
    if (e.shiftKey && !e.altKey) {
      const next = new Set(snap.selectedRegions);
      if (next.has(region.id)) {
        next.delete(region.id);
        const primary = snap.selectedRegion === region.id ? ([...next][0] ?? null) : snap.selectedRegion;
        store.selectRegions(h.track.id, primary, next);
        return;
      }
      next.add(region.id);
      store.selectRegions(h.track.id, region.id, next);
    } else if (!snap.selectedRegions.has(region.id)) {
      store.selectRegion(h.track.id, region.id);
    } else if (snap.selectedRegion !== region.id) {
      store.selectRegions(h.track.id, region.id, snap.selectedRegions);
    }
    if (e.button !== 0) return;
    const mode = h.clip.zone === 'edge' ? 'resize' : h.clip.zone === 'trim' ? 'trim' : 'move';
    const origins = new Map<Id, Origin>();
    const ids = mode === 'move' ? this.selectedIds() : [region.id];
    for (const id of ids) {
      const lane = this.laneOf(id);
      const t = store.getSnapshot().project.tracks[lane];
      const r = t?.regions[id];
      if (r) origins.set(id, { start: r.start, length: r.length, lane });
    }
    this.drag = {
      mode,
      anchorId: region.id,
      origins,
      originX: e.clientX,
      originY: e.clientY,
      downTick: h.tick,
      pointerId: e.pointerId,
      active: false,
      dStart: 0,
      dLane: 0,
      nextLength: region.length,
      copy: false,
    };
    this.d.canvas.setPointerCapture(e.pointerId);
  };

  private onContext = (e: MouseEvent): void => {
    const h = this.hit(e);
    if (!h?.track || !h.clip || !this.d.onMenu) return;
    e.preventDefault();
    const snap = this.d.store.getSnapshot();
    if (!snap.selectedRegions.has(h.clip.region.id)) this.d.store.selectRegion(h.track.id, h.clip.region.id);
    this.d.onMenu({ trackId: h.track.id, regionId: h.clip.region.id, ids: this.selectedIds() }, { x: e.clientX, y: e.clientY });
  };

  /** Shift+F10 or the Menu key: opens the clip menu under the selected clip. */
  private menuFromKeys(): boolean {
    const sel = this.d.actions.selectedRegion();
    if (!sel || !this.d.onMenu) return false;
    const lane = this.laneOf(sel.regionId);
    const region = this.d.store.getSnapshot().project.tracks[lane]?.regions[sel.regionId];
    if (!region) return false;
    const box = this.d.canvas.getBoundingClientRect();
    const view = this.laneView();
    const x = box.left + Math.max(0, (region.start - this.scrollTick) * this.pxPerTick) + 8;
    const y = box.top + view.top + (lane + 1) * view.rowHeight - this.scrollY;
    this.d.onMenu({ trackId: sel.trackId, regionId: sel.regionId, ids: this.selectedIds() }, { x, y: Math.min(y, box.bottom - 8) });
    return true;
  }

  /** What a drag released at this screen point would land on. */
  dropTargetAt(clientX: number, clientY: number): DropTarget {
    const box = this.d.canvas.getBoundingClientRect();
    const at = laneAt(this.laneView(), clientY - box.top);
    const tracks = this.d.store.getSnapshot().project.tracks;
    if (!at) return { kind: 'none' };
    const track = tracks[at.lane];
    const found = track ? hitClip(Object.values(track.regions), this.scrollTick, this.pxPerTick, clientX - box.left, config.regionEdgeGrab) : null;
    return laneTarget(
      tracks.map((t) => t.id),
      at.lane,
      found?.clip.id ?? null,
    );
  }

  private startScrollDrag(e: PointerEvent, px: number): void {
    const span = this.host.width / this.pxPerTick;
    const thumb = scrollThumb(this.scrollTick, span, this.totalTicks(), this.host.width, config.scrollbarMinThumbPx);
    if (!thumb) return;
    const onThumb = px >= thumb.x && px <= thumb.x + thumb.width;
    this.scrollDrag = { pointerId: e.pointerId, grab: onThumb ? px - thumb.x : thumb.width / 2 };
    this.d.canvas.setPointerCapture(e.pointerId);
    this.dragScrollbarTo(px);
  }

  private dragScrollbarTo(px: number): void {
    const g = this.scrollDrag;
    if (!g) return;
    const span = this.host.width / this.pxPerTick;
    const total = this.totalTicks();
    const thumb = scrollThumb(this.scrollTick, span, total, this.host.width, config.scrollbarMinThumbPx);
    if (!thumb) return;
    this.scrollTick = scrollFromThumb(px - g.grab, thumb.width, span, total, this.host.width);
    this.host.invalidate();
  }

  private onMove = (e: PointerEvent): void => {
    const { px, py } = this.point(e);
    if (this.ruler.move(e, px)) return;
    if (this.scrollDrag) return this.dragScrollbarTo(px);
    const d = this.drag;
    if (!d) {
      const onRuler = this.ruler.cursorAt(px, py);
      if (onRuler) return this.setCursor(onRuler);
      if (this.inScrollbar(py)) return this.setCursor('cursor-default');
      const h = this.hit(e);
      this.setCursor(h?.clip ? (h.clip.zone === 'body' ? 'cursor-grab' : 'cursor-ew-resize') : 'cursor-pointer');
      return;
    }
    const dx = e.clientX - d.originX;
    if (!d.active && Math.hypot(dx, e.clientY - d.originY) < config.dragThreshold) return;
    d.active = true;
    const anchor = d.origins.get(d.anchorId);
    if (!anchor) return;
    const step = snapStep(this.sig(), this.pxPerTick, e.shiftKey);
    const dTick = dx / this.pxPerTick;
    if (d.mode === 'resize') {
      const region = this.d.store.getSnapshot().project.tracks[anchor.lane]?.regions[d.anchorId];
      if (region) d.nextLength = resizedLength(region, dTick, step);
      this.setCursor('cursor-ew-resize');
    } else if (d.mode === 'trim') {
      const region = this.d.store.getSnapshot().project.tracks[anchor.lane]?.regions[d.anchorId];
      if (region) d.dStart = clampTrim(region, snapRound(anchor.start + dTick, step) - anchor.start);
      this.setCursor('cursor-ew-resize');
    } else {
      let minStart = Infinity;
      let minLane = Infinity;
      let maxLane = 0;
      for (const o of d.origins.values()) {
        minStart = Math.min(minStart, o.start);
        minLane = Math.min(minLane, o.lane);
        maxLane = Math.max(maxLane, o.lane);
      }
      const snapped = Math.max(0, snapRound(anchor.start + dTick, step));
      d.dStart = Math.max(snapped - anchor.start, -minStart);
      const laneCount = this.d.store.getSnapshot().project.tracks.length;
      const wanted = dropLane(this.laneView(), py) - anchor.lane;
      d.dLane = Math.min(Math.max(wanted, -minLane), laneCount - maxLane);
      d.copy = e.altKey || this.altHeld;
      this.setCursor(d.copy ? 'cursor-copy' : 'cursor-grabbing');
    }
    this.host.invalidate();
  };

  private placements(d: ClipDrag): ClipPlacement[] {
    const tracks = this.d.store.getSnapshot().project.tracks;
    const out: ClipPlacement[] = [];
    for (const [regionId, o] of d.origins) {
      const lane = o.lane + d.dLane;
      out.push({ regionId, trackId: tracks[lane]?.id ?? null, start: o.start + d.dStart });
    }
    return out;
  }

  private endDrag(commit: boolean, e?: PointerEvent): void {
    if (this.scrollDrag) {
      const id = this.scrollDrag.pointerId;
      this.scrollDrag = null;
      if (this.d.canvas.hasPointerCapture(id)) this.d.canvas.releasePointerCapture(id);
      this.host.invalidate();
      return;
    }
    const d = this.drag;
    this.drag = null;
    if (!d) return;
    if (this.d.canvas.hasPointerCapture(d.pointerId)) this.d.canvas.releasePointerCapture(d.pointerId);
    this.setCursor('cursor-pointer');
    this.host.invalidate();
    if (!commit) return;
    if (!d.active) return this.finishClick(d, e);
    const alt = (e?.altKey ?? false) || this.altHeld || d.copy;
    if (d.mode === 'resize') {
      if (d.nextLength !== d.origins.get(d.anchorId)?.length) void this.d.actions.resizeRegion(d.anchorId, d.nextLength);
      return;
    }
    if (d.mode === 'trim') {
      const region = this.d.store.getSnapshot().project.tracks[d.origins.get(d.anchorId)?.lane ?? -1]?.regions[d.anchorId];
      if (region && d.dStart !== 0) void this.d.store.group('Trim clip start', trimStartCommands(region, d.dStart));
      return;
    }
    if (alt) void this.d.actions.copyClips(this.placements(d), d.anchorId);
    else if (d.dStart !== 0 || d.dLane !== 0) void this.d.actions.moveClips(this.placements(d), d.anchorId);
  }

  /** A press without travel: Alt splits at the pointer, a plain click narrows a multi selection to the clip. */
  private finishClick(d: ClipDrag, e?: PointerEvent): void {
    if (e?.altKey || this.altHeld) {
      const step = snapStep(this.sig(), this.pxPerTick, e?.shiftKey ?? false);
      void this.d.actions.splitClips([d.anchorId], snapRound(d.downTick, step));
      return;
    }
    const snap = this.d.store.getSnapshot();
    if (!e?.shiftKey && snap.selectedRegions.size > 1) {
      const lane = this.laneOf(d.anchorId);
      const track = snap.project.tracks[lane];
      if (track) this.d.store.selectRegion(track.id, d.anchorId);
    }
  }

  private onUp = (e: PointerEvent): void => {
    if (!this.ruler.up(e, this.point(e).px)) this.endDrag(true, e);
  };

  private onCancel = (): void => {
    this.ruler.cancel();
    this.endDrag(false);
  };

  private onDouble = (e: MouseEvent): void => {
    const h = this.hit(e);
    if (!h?.track || h.clip) return;
    void this.d.actions.addRegion(h.track.id, snapFloor(Math.max(0, h.tick), this.barTicks()));
  };

  // ---- keyboard: navigate with arrows, edit with Alt, everything the pointer can do ----

  private describe(track: Track, region: Region): string {
    const bars = clipBars(region.start, region.length, this.barTicks());
    return `${track.name}, ${barsText(bars.first, bars.last)}`;
  }

  /** Brings a clip into view and says what it is. */
  private reveal(trackIndex: number, region: Region): void {
    const span = this.host.width / this.pxPerTick;
    if (region.start < this.scrollTick) this.scrollTick = Math.max(0, region.start - span * 0.1);
    else if (region.start + region.length > this.scrollTick + span) {
      this.scrollTick = Math.max(0, Math.min(region.start - span * 0.1, region.start + region.length - span));
    }
    const rh = trackRowHeight();
    const y = trackIndex * rh;
    const viewH = this.host.height - config.rulerHeight;
    if (y < this.scrollY) this.d.scrollListBy(y - this.scrollY);
    else if (y + rh > this.scrollY + viewH) this.d.scrollListBy(y + rh - (this.scrollY + viewH));
    this.host.invalidate();
  }

  private focusClip(trackIndex: number, region: Region, extend: boolean): void {
    const store = this.d.store;
    const snap = store.getSnapshot();
    const track = snap.project.tracks[trackIndex];
    if (!track) return;
    if (extend) store.selectRegions(track.id, region.id, new Set([...snap.selectedRegions, region.id]));
    else store.selectRegion(track.id, region.id);
    this.reveal(trackIndex, region);
    const count = store.getSnapshot().selectedRegions.size;
    store.say(count > 1 ? `${this.describe(track, region)}, ${count} clips selected` : this.describe(track, region));
  }

  private navigate(key: string, extend: boolean): void {
    const snap = this.d.store.getSnapshot();
    const tracks = snap.project.tracks;
    if (tracks.length === 0) return;
    const current = snap.selectedRegion ? tracks.findIndex((t) => t.regions[snap.selectedRegion ?? ''] !== undefined) : -1;
    const lane = current >= 0 ? current : Math.max(0, tracks.findIndex((t) => t.id === snap.selectedTrack));
    const here = current >= 0 ? tracks[current]?.regions[snap.selectedRegion ?? ''] : undefined;
    const clips = (i: number): Region[] => Object.values(tracks[i]?.regions ?? {});
    if (key === 'ArrowLeft' || key === 'ArrowRight') {
      const own = clips(lane);
      const target = here
        ? neighbourClip(own, here.id, key === 'ArrowRight' ? 1 : -1)
        : own.length > 0
          ? [...own].sort((a, b) => a.start - b.start)[key === 'ArrowRight' ? 0 : own.length - 1]
          : null;
      const region = target ? tracks[lane]?.regions[target.id] : undefined;
      if (region) this.focusClip(lane, region, extend);
      else this.d.store.say(key === 'ArrowRight' ? 'No later clip on this track' : 'No earlier clip on this track');
      return;
    }
    const next = lane + (key === 'ArrowDown' ? 1 : -1);
    const track = tracks[next];
    if (!track) {
      this.d.store.say(key === 'ArrowDown' ? 'Last track' : 'First track');
      return;
    }
    const near = clipNearTick(Object.values(track.regions), here?.start ?? Math.round(this.scrollTick));
    const region = near ? track.regions[near.id] : undefined;
    if (region) return this.focusClip(next, region, extend);
    this.d.store.selectRegion(track.id, null);
    this.d.store.say(`${track.name}, no clips`);
  }

  private editWithKeys(e: KeyboardEvent): void {
    const snap = this.d.store.getSnapshot();
    const anchorId = snap.selectedRegion;
    const lane = anchorId ? this.laneOf(anchorId) : -1;
    const anchor = lane >= 0 ? snap.project.tracks[lane]?.regions[anchorId ?? ''] : undefined;
    if (!anchor || !anchorId) return;
    const step = snapStep(this.sig(), this.pxPerTick, false);
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      if (e.shiftKey) {
        const length = resizedLength(anchor, dir * step, step);
        if (length !== anchor.length) void this.d.actions.resizeRegion(anchorId, length);
        return;
      }
      const ids = this.selectedIds();
      const minStart = Math.min(...ids.map((id) => snap.project.tracks[this.laneOf(id)]?.regions[id]?.start ?? 0));
      const dStart = Math.max(movedStart(anchor, dir * step, step) - anchor.start, -minStart);
      void this.d.actions.moveClips(this.shifted(ids, dStart, 0), anchorId);
      return;
    }
    const dLane = e.key === 'ArrowDown' ? 1 : -1;
    const ids = this.selectedIds();
    const lanes = ids.map((id) => this.laneOf(id));
    if (Math.min(...lanes) + dLane < 0 || Math.max(...lanes) + dLane > snap.project.tracks.length) return;
    void this.d.actions.moveClips(this.shifted(ids, 0, dLane), anchorId);
  }

  private shifted(ids: readonly Id[], dStart: number, dLane: number): ClipPlacement[] {
    const tracks = this.d.store.getSnapshot().project.tracks;
    const out: ClipPlacement[] = [];
    for (const regionId of ids) {
      const lane = this.laneOf(regionId);
      const r = tracks[lane]?.regions[regionId];
      if (r) out.push({ regionId, trackId: tracks[lane + dLane]?.id ?? null, start: r.start + dStart });
    }
    return out;
  }

  private async addClipHere(): Promise<void> {
    const snap = this.d.store.getSnapshot();
    const track = snap.project.tracks.find((t) => t.id === snap.selectedTrack) ?? snap.project.tracks[0];
    if (!track) return;
    const sel = snap.selectedRegion ? track.regions[snap.selectedRegion] : undefined;
    const start = sel ? sel.start + sel.length : snapFloor(Math.round(this.playTick()), this.barTicks());
    const id = await this.d.actions.addRegion(track.id, start);
    const made = id ? this.d.store.getSnapshot().project.tracks.find((t) => t.id === track.id)?.regions[id] : undefined;
    if (made) this.d.store.say(this.describe(track, made));
  }

  private onKeyUp = (e: KeyboardEvent): void => {
    if (e.key !== 'Alt') return;
    this.altHeld = false;
    if (this.drag?.active) {
      this.drag.copy = false;
      this.host.invalidate();
    }
  };

  private onBlur = (): void => {
    this.altHeld = false;
  };

  private onKey = (e: KeyboardEvent): void => {
    if (e.key === 'Alt') {
      this.altHeld = true;
      if (this.drag?.active && this.drag.mode === 'move') {
        this.drag.copy = true;
        this.setCursor('cursor-copy');
        this.host.invalidate();
      }
      return;
    }
    if (isMenuKey(e) && this.menuFromKeys()) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (e.key === 'Escape' && this.ruler.active) {
      this.ruler.cancel();
      return;
    }
    if (e.key === 'Escape' && this.drag) {
      e.preventDefault();
      this.endDrag(false);
      return;
    }
    const mod = e.ctrlKey || e.metaKey;
    // Ctrl and Cmd combos other than select all belong to the app (undo) or to later clipboard work.
    if (mod) {
      this.modKey(e);
      return;
    }
    const key = e.key;
    if (key.startsWith('Arrow')) {
      e.preventDefault();
      if (e.altKey) this.editWithKeys(e);
      else this.navigate(key, e.shiftKey);
      return;
    }
    const handled = this.letterKey(e);
    if (handled) {
      e.preventDefault();
      // Delete would otherwise reach the global handler, which deletes notes.
      e.stopPropagation();
    }
  };

  /** Ctrl or Cmd A, C, X, V, D and Shift D (Repeat x N); other combos belong to the app. */
  private modKey(e: KeyboardEvent): void {
    const a = this.d.actions;
    const key = e.key.toLowerCase();
    if (key === 'a' && !e.shiftKey) this.selectAll();
    else if (key === 'c' && !e.shiftKey) a.copyClipsToClipboard(this.selectedIds());
    else if (key === 'x' && !e.shiftKey) void a.cutClips(this.selectedIds());
    else if (key === 'v' && !e.shiftKey) void a.pasteInArrangement(this.playTick());
    else if (key === 'd' && e.shiftKey) void a.repeatClipsAsk(this.selectedIds());
    else if (key === 'd') void a.duplicateClips(this.selectedIds());
    else return;
    e.preventDefault();
  }

  /** Toolbar and palette entry: duplicate or repeat the selected clips. */
  duplicateSelection(): void {
    void this.d.actions.duplicateClips(this.selectedIds());
  }

  repeatSelection(): void {
    void this.d.actions.repeatClipsAsk(this.selectedIds());
  }

  private letterKey(e: KeyboardEvent): boolean {
    const snap = this.d.store.getSnapshot();
    switch (e.key) {
      case 'Enter':
        if (!snap.selectedRegion) return false;
        this.d.openClip(snap.selectedRegion);
        return true;
      case 'Delete':
      case 'Backspace':
        if (snap.selectedRegions.size === 0) return false;
        void this.d.actions.deleteClips(this.selectedIds());
        return true;
      case 'Home':
        this.home();
        return true;
      case 'PageUp':
      case 'PageDown': {
        const span = this.host.width / this.pxPerTick;
        this.scrollTick += (e.key === 'PageDown' ? 1 : -1) * span * 0.9;
        this.host.invalidate();
        return true;
      }
      case '+':
      case '=':
        this.zoomBy(config.zoomStep);
        return true;
      case '-':
        this.zoomBy(1 / config.zoomStep);
        return true;
      default:
        break;
    }
    switch (e.key.toLowerCase()) {
      case 's':
        void this.splitAtPlayhead();
        return true;
      case 'f':
        this.fit(!e.shiftKey);
        return true;
      case 'n':
        void this.addClipHere();
        return true;
      default:
        return false;
    }
  }

  private selectAll(): void {
    const snap = this.d.store.getSnapshot();
    const ids: Id[] = [];
    for (const t of snap.project.tracks) ids.push(...Object.keys(t.regions));
    if (ids.length === 0) return;
    const track = snap.project.tracks.find((t) => t.id === snap.selectedTrack) ?? snap.project.tracks[0];
    if (!track) return;
    this.d.store.selectRegions(track.id, snap.selectedRegion ?? ids[0] ?? null, ids);
    this.d.store.say(`${ids.length} clips selected`);
  }

  // ---- wheel ----

  private onWheel = (e: WheelEvent): void => {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      this.zoomBy(Math.exp(-e.deltaY * 0.01), e.clientX - this.d.canvas.getBoundingClientRect().left);
      return;
    } else if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      this.scrollTick += (e.shiftKey && e.deltaX === 0 ? e.deltaY : e.deltaX) / this.pxPerTick;
    } else {
      this.d.scrollListBy(e.deltaY);
    }
    this.host.invalidate();
  };

  // ---- debug ----

  private probe(): ArrangementProbe {
    const rect = this.d.canvas.getBoundingClientRect();
    const rh = trackRowHeight();
    const snap = this.d.store.getSnapshot();
    const clips: ArrangementProbe['clips'] = [];
    snap.project.tracks.forEach((t, lane) => {
      for (const r of Object.values(t.regions)) {
        if (clips.length >= config.probeMaxClips) return;
        clips.push({
          id: r.id,
          track: t.id,
          lane,
          start: r.start,
          length: r.length,
          x: rect.left + (r.start - this.scrollTick) * this.pxPerTick,
          y: rect.top + config.rulerHeight + lane * rh - this.scrollY + 3,
          width: r.length * this.pxPerTick,
          height: rh - 7,
        });
      }
    });
    return {
      pxPerTick: this.pxPerTick,
      scrollTick: this.scrollTick,
      canvas: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
      rulerHeight: config.rulerHeight,
      rowHeight: rh,
      clips,
      selected: [...snap.selectedRegions],
    };
  }
}
