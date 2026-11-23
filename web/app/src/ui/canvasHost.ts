// Shared canvas plumbing: device pixel ratio sizing, one rAF loop for every canvas that draws
// only hosts that are dirty (or while the playhead moves), and cleanup. React never sees a frame.
import { recordFrame } from '../debug';

/** One requestAnimationFrame loop serves every host, so two canvases cost one callback. */
class FrameService {
  private readonly hosts = new Set<CanvasHost>();
  private raf = 0;

  add(host: CanvasHost): void {
    this.hosts.add(host);
    if (!this.raf) this.raf = requestAnimationFrame(this.tick);
  }

  remove(host: CanvasHost): void {
    this.hosts.delete(host);
    if (this.hosts.size === 0 && this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
  }

  private tick = (): void => {
    this.raf = requestAnimationFrame(this.tick);
    for (const h of this.hosts) h.frame();
  };
}

const frames = new FrameService();

export interface HostOptions {
  canvas: HTMLCanvasElement;
  /** Called with a context already scaled to CSS pixels. */
  draw(ctx: CanvasRenderingContext2D, width: number, height: number): void;
  /** True while the view moves on its own (playhead), so frames keep coming. */
  animating(): boolean;
  /** Record draw time in the debug stats. */
  measure?: boolean;
  /** Receives the draw time in ms of every frame this host paints. */
  onDraw?(ms: number): void;
}

export class CanvasHost {
  width = 0;
  height = 0;
  private dpr = 1;
  private dirty = true;
  private readonly ctx: CanvasRenderingContext2D;
  private readonly observer: ResizeObserver;

  constructor(private readonly opts: HostOptions) {
    const ctx = opts.canvas.getContext('2d', { alpha: false });
    if (!ctx) throw new Error('2d canvas is not available');
    this.ctx = ctx;
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(opts.canvas);
    this.resize();
    frames.add(this);
  }

  invalidate(): void {
    this.dirty = true;
  }

  private resize(): void {
    const { canvas } = this.opts;
    const rect = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(rect.width));
    const h = Math.max(1, Math.round(rect.height));
    if (w === this.width && h === this.height && dpr === this.dpr) return;
    this.width = w;
    this.height = h;
    this.dpr = dpr;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    this.dirty = true;
  }

  /** Called by the shared loop once per animation frame. */
  frame(): void {
    if (!this.dirty && !this.opts.animating()) return;
    this.dirty = false;
    const t0 = performance.now();
    this.ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    this.opts.draw(this.ctx, this.width, this.height);
    const ms = performance.now() - t0;
    if (this.opts.measure) recordFrame(ms);
    this.opts.onDraw?.(ms);
  }

  destroy(): void {
    frames.remove(this);
    this.observer.disconnect();
  }
}
