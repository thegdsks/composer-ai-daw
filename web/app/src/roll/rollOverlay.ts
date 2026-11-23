// Canvas drawing for the roll's beginner aids: key shading, the drag tooltip and the octave
// readout. Pure drawing, no state, so RollController only decides when to call it.
import { config } from '../config';
import { pitchName } from '../time';
import type { Palette } from '../ui/palette';
import { inKey, type KeyInfo } from './keyScale';
import { MAX_PITCH, pitchToY, yToPitch, type Viewport } from './geometry';

const TIP_PAD = 6;
const TIP_OFFSET = 14;
const TIP_HEIGHT = 20;

/** "bar 3 beat 2" for an absolute tick; beats count from 1. */
export function positionText(tick: number, ticksPerBar: number): string {
  const t = Math.max(0, tick);
  const bar = Math.floor(t / ticksPerBar) + 1;
  const beat = Math.floor((t % ticksPerBar) / config.ticksPerQuarter) + 1;
  return `bar ${bar} beat ${beat}`;
}

/** Dims rows outside the key and tints the key note rows, behind the notes. */
export function drawKeyRows(
  ctx: CanvasRenderingContext2D,
  pal: Palette,
  vp: Viewport,
  mask: readonly boolean[],
  key: KeyInfo,
  gw: number,
  gh: number,
): void {
  const top = yToPitch(vp, 0);
  const bot = yToPitch(vp, gh);
  for (let p = bot; p <= top; p++) {
    const y = pitchToY(vp, p);
    if (!inKey(mask, p)) {
      ctx.globalAlpha = 0.6;
      ctx.fillStyle = pal.outKey;
      ctx.fillRect(0, y, gw, vp.rowHeight);
    } else if (p % 12 === key.root) {
      ctx.globalAlpha = 0.14;
      ctx.fillStyle = pal.accent;
      ctx.fillRect(0, y, gw, vp.rowHeight);
    }
  }
  ctx.globalAlpha = 1;
}

/** A small label that follows the pointer while a note is dragged. */
export function drawTooltip(
  ctx: CanvasRenderingContext2D,
  pal: Palette,
  text: string,
  x: number,
  y: number,
  w: number,
  h: number,
): void {
  ctx.font = '12px ui-sans-serif, system-ui, sans-serif';
  ctx.textBaseline = 'middle';
  const bw = ctx.measureText(text).width + TIP_PAD * 2;
  const bx = Math.min(Math.max(0, x + TIP_OFFSET), Math.max(0, w - bw));
  const by = Math.min(Math.max(0, y - TIP_HEIGHT - TIP_OFFSET / 2), Math.max(0, h - TIP_HEIGHT));
  ctx.fillStyle = pal.panel;
  ctx.fillRect(bx, by, bw, TIP_HEIGHT);
  ctx.fillStyle = pal.bar;
  ctx.fillRect(bx, by, bw, 1);
  ctx.fillRect(bx, by + TIP_HEIGHT - 1, bw, 1);
  ctx.fillStyle = pal.ink;
  ctx.fillText(text, bx + TIP_PAD, by + TIP_HEIGHT / 2);
}

/** Octave range in view, drawn in the corner above the keyboard. */
export function drawOctaveReadout(
  ctx: CanvasRenderingContext2D,
  pal: Palette,
  vp: Viewport,
  gh: number,
  keyboardWidth: number,
  rulerHeight: number,
): void {
  const hi = Math.min(MAX_PITCH, yToPitch(vp, 0));
  const lo = Math.max(0, yToPitch(vp, gh));
  const text = `${pitchName(lo - (lo % 12))} to ${pitchName(Math.min(MAX_PITCH, hi - (hi % 12)))}`;
  ctx.font = '10px ui-sans-serif, system-ui, sans-serif';
  ctx.textBaseline = 'middle';
  ctx.fillStyle = pal.inkDim;
  ctx.fillText(text, 4, rulerHeight / 2, keyboardWidth - 8);
}
