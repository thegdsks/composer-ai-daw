// Timeline lanes above the arrangement: markers, tempo changes and key changes on one canvas,
// sharing the arrangement's scroll and zoom. Drawing only; a gesture is turned into a request
// that the panel answers (a dialog or a command), so the lanes hold no edit logic.
import type { AudioEngine } from '../audio/audioEngine';
import { config } from '../config';
import type { Tick } from '../model';
import { LANE_LABELS, type LaneId } from '../state/laneStore';
import { keyPoints, markerPoints, tempoPoints, hitPoint, type LanePoint } from '../state/timelinePlans';
import type { Store } from '../state/store';
import { barLines, secondsToTicks, snapRound } from '../time';
import { CanvasHost } from '../ui/canvasHost';
import type { Palette } from '../ui/palette';
import { keyName } from './keyScale';
import { snapStep } from './regionDrag';

export interface LaneRequest {
  lane: LaneId;
  /** Tick of the existing point that was hit, or null on empty space. */
  point: Tick | null;
  /** Snapped tick under the pointer. */
  at: Tick;
}

export interface LaneMenuRequest extends LaneRequest {
  x: number;
  y: number;
}

export interface TimelineLaneDeps {
  canvas: HTMLCanvasElement;
  store: Store;
  audio: AudioEngine;
  view(): { scrollTick: number; pxPerTick: number };
  lanes(): readonly LaneId[];
  getPalette(): Palette | null;
  /** Double click: empty space adds, a point edits. */
  onEdit(req: LaneRequest): void;
  onMenu(req: LaneMenuRequest): void;
  /** Single click on a point moves the playhead there. */
  seek(tick: Tick): void;
}

export class TimelineLanes {
  private readonly host: CanvasHost;
  private lastKey = '';
  private readonly unsub: () => void;

  constructor(private readonly d: TimelineLaneDeps) {
    this.host = new CanvasHost({
      canvas: d.canvas,
      draw: (ctx, w, h) => this.draw(ctx, w, h),
      animating: () => this.drawKey() !== this.lastKey,
    });
    d.canvas.addEventListener('dblclick', this.onDouble);
    d.canvas.addEventListener('click', this.onClick);
    d.canvas.addEventListener('contextmenu', this.onContext);
    this.unsub = d.store.subscribe(() => this.host.invalidate());
  }

  destroy(): void {
    this.d.canvas.removeEventListener('dblclick', this.onDouble);
    this.d.canvas.removeEventListener('click', this.onClick);
    this.d.canvas.removeEventListener('contextmenu', this.onContext);
    this.unsub();
    this.host.destroy();
  }

  invalidate(): void {
    this.host.invalidate();
  }

  /** What the picture depends on besides the project: view, lane set and playhead. */
  private drawKey(): string {
    const v = this.d.view();
    return `${v.scrollTick}|${v.pxPerTick}|${this.d.lanes().join(',')}|${this.playTick()}`;
  }

  private playTick(): Tick {
    const s = this.d.audio.status;
    if (s.sampleRate <= 0) return 0;
    return Math.round(secondsToTicks(this.d.store.getSnapshot().project.tempo_map, s.positionFrames / s.sampleRate));
  }

  private pointsOf(lane: LaneId): LanePoint[] {
    const p = this.d.store.getSnapshot().project;
    if (lane === 'markers') return markerPoints(p);
    if (lane === 'tempo') return tempoPoints(p.tempo_map);
    return keyPoints(p.tempo_map, (root, scale) => keyName({ root, scale }));
  }

  private draw(ctx: CanvasRenderingContext2D, w: number, h: number): void {
    const pal = this.d.getPalette();
    if (!pal) return;
    this.lastKey = this.drawKey();
    const { scrollTick, pxPerTick } = this.d.view();
    const x = (t: Tick): number => (t - scrollTick) * pxPerTick;
    const map = this.d.store.getSnapshot().project.tempo_map;
    const lanes = this.d.lanes();
    const rh = config.laneHeight;
    ctx.fillStyle = pal.bg;
    ctx.fillRect(0, 0, w, h);
    const lines = barLines(map, scrollTick, scrollTick + w / pxPerTick, false);
    ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
    ctx.textBaseline = 'middle';
    lanes.forEach((lane, i) => {
      const y = i * rh;
      ctx.fillStyle = i % 2 === 0 ? pal.row : pal.bg;
      ctx.fillRect(0, y, w, rh);
      ctx.fillStyle = pal.line;
      for (const l of lines) ctx.fillRect(Math.round(x(l.tick)), y, 1, rh);
      ctx.fillRect(0, y + rh - 1, w, 1);
      const points = this.pointsOf(lane);
      const from = scrollTick - 400 / pxPerTick;
      const to = scrollTick + w / pxPerTick;
      for (let k = 0; k < points.length; k++) {
        const p = points[k]!;
        if (p.tick < from || p.tick > to) continue;
        const px = Math.round(x(p.tick));
        const next = points[k + 1];
        const room = next ? x(next.tick) - px - 12 : w - px;
        this.drawPoint(ctx, pal, lane, px, y, rh, p.label, room);
      }
    });
    const ph = x(this.playTick());
    if (ph >= 0 && ph <= w) {
      ctx.fillStyle = pal.playhead;
      ctx.fillRect(Math.round(ph), 0, 2, h);
    }
  }

  private drawPoint(ctx: CanvasRenderingContext2D, pal: Palette, lane: LaneId, px: number, y: number, rh: number, label: string, room: number): void {
    const mid = y + rh / 2;
    ctx.fillStyle = pal.ink;
    if (lane === 'markers') {
      // A flag: pole and pennant.
      ctx.fillRect(px, y + 3, 1, rh - 6);
      ctx.fillRect(px, y + 3, 6, 5);
    } else {
      ctx.beginPath();
      ctx.moveTo(px, mid - 5);
      ctx.lineTo(px + 5, mid);
      ctx.lineTo(px, mid + 5);
      ctx.lineTo(px - 5, mid);
      ctx.closePath();
      ctx.fill();
    }
    if (room < 16) return;
    ctx.save();
    ctx.beginPath();
    ctx.rect(px + 8, y, Math.max(0, room - 8), rh);
    ctx.clip();
    ctx.fillStyle = pal.inkDim;
    ctx.fillText(lane === 'tempo' ? `${label} BPM` : label, px + 9, mid);
    ctx.restore();
  }

  // ---- gestures ----

  private where(e: { clientX: number; clientY: number }): { lane: LaneId; px: number } | null {
    const box = this.d.canvas.getBoundingClientRect();
    const px = e.clientX - box.left;
    const lane = this.d.lanes()[Math.floor((e.clientY - box.top) / config.laneHeight)];
    return lane ? { lane, px } : null;
  }

  private request(e: { clientX: number; clientY: number }): LaneRequest | null {
    const at = this.where(e);
    if (!at) return null;
    const { scrollTick, pxPerTick } = this.d.view();
    const points = this.pointsOf(at.lane);
    const hit = hitPoint(points, at.px, scrollTick, pxPerTick);
    const map = this.d.store.getSnapshot().project.tempo_map;
    const sig = Object.values(map.meters)[0] ?? { numerator: 4, denominator: 4 };
    const step = snapStep(sig, pxPerTick, false);
    const tick = Math.max(0, snapRound(at.px / pxPerTick + scrollTick, step));
    return { lane: at.lane, point: hit >= 0 ? points[hit]!.tick : null, at: tick };
  }

  private onClick = (e: MouseEvent): void => {
    const r = this.request(e);
    if (r && r.point !== null) this.d.seek(r.point);
  };

  private onDouble = (e: MouseEvent): void => {
    const r = this.request(e);
    if (r) this.d.onEdit(r);
  };

  private onContext = (e: MouseEvent): void => {
    const r = this.request(e);
    if (!r || r.point === null) return;
    e.preventDefault();
    this.d.onMenu({ ...r, x: e.clientX, y: e.clientY });
  };
}

export function laneLabel(lane: LaneId): string {
  return LANE_LABELS[lane];
}
