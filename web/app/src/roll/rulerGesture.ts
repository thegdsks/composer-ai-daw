// Pointer gestures on the arrangement ruler: a click seeks, a drag draws the loop brace, a drag
// from a brace edge moves that edge. Pure of drawing; the controller paints the preview.
import { config } from '../config';
import { braceFromDrag, rulerTick } from '../time';

export interface RulerDeps {
  canvas: HTMLCanvasElement;
  /** Visible window: first tick on screen and zoom. */
  view(): { scrollTick: number; pxPerTick: number };
  loop(): { on: boolean; start: number; end: number };
  /** Snap step in ticks for the current zoom; `free` is one tick. */
  step(free: boolean): number;
  seek(tick: number): void;
  setLoop(start: number, end: number): void;
  invalidate(): void;
}

type Mode = 'new' | 'start' | 'end';

interface Active {
  pointerId: number;
  mode: Mode;
  downPx: number;
  moved: boolean;
}

export class RulerGesture {
  private cur: Active | null = null;
  /** Brace being dragged, drawn in place of the saved one until release. */
  preview: { start: number; end: number } | null = null;

  constructor(private readonly d: RulerDeps) {}

  get active(): boolean {
    return this.cur !== null;
  }

  private edgeAt(px: number): 'start' | 'end' | null {
    const loop = this.d.loop();
    if (loop.end <= loop.start) return null;
    const { scrollTick, pxPerTick } = this.d.view();
    const grab = config.regionEdgeGrab;
    const a = Math.abs(px - (loop.start - scrollTick) * pxPerTick);
    const b = Math.abs(px - (loop.end - scrollTick) * pxPerTick);
    if (Math.min(a, b) > grab) return null;
    return a <= b ? 'start' : 'end';
  }

  /** Cursor class for the ruler at a point, or null when the point is not on the ruler. */
  cursorAt(px: number, py: number): 'cursor-ew-resize' | 'cursor-pointer' | null {
    if (py >= config.rulerHeight) return null;
    return this.edgeAt(px) ? 'cursor-ew-resize' : 'cursor-pointer';
  }

  down(e: PointerEvent, px: number, py: number): boolean {
    if (e.button !== 0 || py >= config.rulerHeight) return false;
    this.cur = { pointerId: e.pointerId, mode: this.edgeAt(px) ?? 'new', downPx: px, moved: false };
    this.d.canvas.setPointerCapture(e.pointerId);
    return true;
  }

  move(e: PointerEvent, px: number): boolean {
    const c = this.cur;
    if (!c) return false;
    if (!c.moved && Math.abs(px - c.downPx) < config.dragThreshold) return true;
    c.moved = true;
    const { scrollTick, pxPerTick } = this.d.view();
    const step = this.d.step(e.shiftKey);
    const at = rulerTick(px, scrollTick, pxPerTick, step);
    const loop = this.d.loop();
    if (c.mode === 'new') {
      this.preview = braceFromDrag(rulerTick(c.downPx, scrollTick, pxPerTick, step), at, step);
    } else if (c.mode === 'start') {
      this.preview = braceFromDrag(Math.min(at, loop.end - step), loop.end, step);
    } else {
      this.preview = braceFromDrag(loop.start, Math.max(at, loop.start + step), step);
    }
    this.d.invalidate();
    return true;
  }

  up(e: PointerEvent, px: number): boolean {
    const c = this.cur;
    if (!c) return false;
    this.cur = null;
    if (this.d.canvas.hasPointerCapture(c.pointerId)) this.d.canvas.releasePointerCapture(c.pointerId);
    const brace = this.preview;
    this.preview = null;
    this.d.invalidate();
    if (!c.moved) {
      const { scrollTick, pxPerTick } = this.d.view();
      this.d.seek(rulerTick(px, scrollTick, pxPerTick, this.d.step(e.shiftKey)));
    } else if (brace) {
      this.d.setLoop(brace.start, brace.end);
    }
    return true;
  }

  cancel(): void {
    const c = this.cur;
    if (!c) return;
    this.cur = null;
    this.preview = null;
    if (this.d.canvas.hasPointerCapture(c.pointerId)) this.d.canvas.releasePointerCapture(c.pointerId);
    this.d.invalidate();
  }
}
