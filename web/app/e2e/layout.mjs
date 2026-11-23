// U38 exit: at 1280x800 the piano roll shows enough pitch rows, the top bar is one row, and the key
// controls stay reachable and uncovered. Run: node e2e/run.mjs layout
// Env: E2E_MIN_ROWS (default 14), E2E_MAX_BAR_PX (default 60).
import { assert, envNum } from './lib.mjs';
import { ev, sleep } from './clip-lib.mjs';

// True when the element is inside the viewport and the topmost thing at its center is itself or a child.
const reachable = (sel) => `(() => {
  const el = document.querySelector(${JSON.stringify(sel)});
  if (!el) return { found: false };
  const r = el.getBoundingClientRect();
  const x = Math.round(r.left + r.width / 2);
  const y = Math.round(r.top + r.height / 2);
  const top = document.elementFromPoint(x, y);
  return { found: true, inside: r.width > 0 && r.height > 0 && r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight, hit: !!top && (top === el || el.contains(top)), box: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)] };
})()`;

async function check(page, sel, what) {
  const r = ev(page, reachable(sel));
  assert(r.found, `${what}: ${sel} not found`);
  assert(r.inside, `${what} is outside the 1280x800 viewport: ${JSON.stringify(r.box)}`);
  assert(r.hit, `${what} is covered by another element at ${JSON.stringify(r.box)}`);
  return r.box;
}

export default async function layout({ page, url }) {
  const minRows = envNum('E2E_MIN_ROWS', 14);
  const maxBar = envNum('E2E_MAX_BAR_PX', 60);
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off`);
  await page.waitDebug((d) => d.project && d.project.tracks >= 2 && d.arrangement && d.arrangement.clips.length >= 2, 'starter project');
  await sleep(600);

  const out = {};
  const bar = ev(page, `document.querySelector('header').getBoundingClientRect().height`);
  assert(bar <= maxBar, `the top bar wraps: ${bar}px tall, limit ${maxBar}`);
  out.barPx = Math.round(bar);

  const rows = Number(ev(page, `document.getElementById('piano-roll').dataset.rows`));
  assert(rows >= minRows, `the piano roll shows ${rows} pitch rows, need ${minRows}`);
  out.rows = rows;
  out.lane = ev(page, `document.getElementById('piano-roll').dataset.lane`);

  await check(page, '[aria-label="Transport"] button', 'Play');
  await check(page, '[data-testid="record"]', 'Record');
  await check(page, '[data-testid="dock-tab-mix"]', 'Mix tab');
  await check(page, '[data-testid="dock-tab-notes"]', 'Notes tab');

  // Loudness lane: folding it gives rows back and the choice is remembered.
  const toggle = '[data-testid="velocity-lane-toggle"]';
  const before = ev(page, `document.querySelector('${toggle}').getAttribute('aria-pressed')`);
  page.run(['eval', `document.querySelector('${toggle}').click()`]);
  await sleep(400);
  const flipped = ev(page, `document.querySelector('${toggle}').getAttribute('aria-pressed')`);
  assert(flipped !== before, 'the Loudness toggle did not change state');
  if (before === 'true') {
    const more = Number(ev(page, `document.getElementById('piano-roll').dataset.rows`));
    assert(more > rows, `folding the lane should add rows: ${rows} then ${more}`);
    out.rowsLaneFolded = more;
  }
  const stored = ev(page, `JSON.parse(localStorage.getItem(${JSON.stringify('composer.prefs.v1')}) || '{}').velocityLane ?? null`);
  assert(stored === 'open' || stored === 'closed', `the lane choice was not remembered: ${stored}`);
  page.run(['eval', `document.querySelector('${toggle}').click()`]);
  await sleep(300);

  // The Mix tab: the mute button of the last strip is not under the dock edge or the status bar.
  page.run(['eval', `document.querySelector('[data-testid="dock-tab-mix"]').click()`]);
  await sleep(600);
  out.mute = await check(page, '[data-testid="strip-mute-1"]', 'Mixer mute button');
  await check(page, '[data-testid="strip-solo-1"]', 'Mixer solo button');

  page.run(['eval', `document.querySelector('[data-testid="dock-tab-notes"]').click()`]);
  await sleep(300);
  return out;
}
