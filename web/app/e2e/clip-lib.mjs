// Helpers shared by the clip e2e scripts: read the arrangement probe, drive the pointer and keys.
import { assert } from './lib.mjs';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export { sleep };

/** Runs JS in the page and returns the parsed JSON result (the expression must be JSON-able). */
export function ev(page, expr) {
  const out = page.run(['eval', `JSON.stringify(${expr})`]).trim();
  let v = JSON.parse(out);
  if (typeof v === 'string') v = JSON.parse(v);
  return v;
}

export async function boot(page, url) {
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off`);
  await page.waitDebug((d) => d.project && d.project.tracks >= 2 && d.arrangement && d.arrangement.clips.length >= 2, 'starter project and arrangement');
}

export const state = (page) => {
  const d = page.debug();
  return { hash: d.project.hash, depth: d.undoDepth, arr: d.arrangement, clips: d.arrangement.clips };
};

export const clipCenter = (c, dx = 0) => ({ x: Math.round(c.x + c.width / 2 + dx), y: Math.round(c.y + c.height / 2) });

export function press(page, key) {
  page.run(['press', key]);
}

/** Moves the pointer along a straight path in a few steps so move events fire like a real drag. */
export function dragTo(page, from, to, { alt = false, shift = false, steps = 6 } = {}) {
  page.run(['mouse', 'move', String(from.x), String(from.y)]);
  if (shift) page.run(['keydown', 'Shift']);
  page.run(['mouse', 'down']);
  for (let i = 1; i <= steps; i++) {
    page.run(['mouse', 'move', String(Math.round(from.x + ((to.x - from.x) * i) / steps)), String(Math.round(from.y + ((to.y - from.y) * i) / steps))]);
  }
  if (alt) page.run(['keydown', 'Alt']);
  page.run(['mouse', 'up']);
  if (alt) page.run(['keyup', 'Alt']);
  if (shift) page.run(['keyup', 'Shift']);
}

/** Waits until the undo depth reaches `depth`, so a commit has landed before the next read. */
export async function waitDepth(page, depth) {
  await page.waitDebug((d) => d.undoDepth === depth, `undo depth ${depth}`);
}

export async function announcer(page, matcher, what) {
  const deadline = Date.now() + 5000;
  for (;;) {
    const text = ev(page, `document.querySelector('[data-testid="announcer"]').textContent`);
    if (matcher(text)) return text;
    assert(Date.now() < deadline, `announcer never said ${what}; last text: "${text}"`);
    await sleep(100);
  }
}
