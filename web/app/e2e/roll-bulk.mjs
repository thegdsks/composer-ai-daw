// U17 exit: marquee 200 notes, drag them up 2 semitones, one undo step, undo restores the hash.
// Then the note list: windowed rows, and a loudness edit from the keyboard is one undo step.
// Run: node e2e/run.mjs roll-bulk
import { assert } from './lib.mjs';
import { dragTo, ev, press, sleep, waitDepth } from './clip-lib.mjs';

const RULER = 28; // VITE_RULER_HEIGHT
const KEYBOARD = 52; // VITE_KEYBOARD_WIDTH
const VELOCITY_LANE = 60; // VITE_VELOCITY_LANE
const COUNT = 200;
const STRESS_LOW = 36; // src/fixtures.ts, first note of the stress project is the first LCG draw
const FIRST_PITCH = STRESS_LOW + ((7 * 1103515245 + 12345) % 2 ** 31) % 60;

function view(page) {
  return ev(
    page,
    `(() => { const c = document.getElementById('piano-roll'); const r = c.getBoundingClientRect(); return { l: r.left, t: r.top, w: r.width, h: r.height, v: c.dataset.view.split(',').map(Number) }; })()`,
  );
}

const status = (page) => ev(page, `document.querySelector('[data-testid="roll-status"]').textContent`);

export default async function rollBulk({ page, url }) {
  page.run(['set', 'viewport', '1400', '1700']);
  page.open(`${url}?e2e=1&start=off&stress=${COUNT}`);
  await page.waitDebug((d) => d.project && d.project.notes >= COUNT, `${COUNT} notes`, 60000);
  await sleep(800);
  const out = {};
  // Give the dock most of the height so all 60 pitch rows of the stress clip are on screen.
  page.run(['focus', page.findRef('Resize the arrangement')]);
  for (let i = 0; i < 12; i++) press(page, 'ArrowUp');
  page.click('Edit tools');
  await sleep(400);
  ev(page, `document.querySelector('button[title="Frame the clip (Home)"]').click() || 1`);
  // The clip is longer than the widest fit, so zoom out until all of it is on screen.
  for (let i = 0; i < 2; i++) ev(page, `document.querySelector('button[aria-label="Zoom out (minus)"]').click() || 1`);
  await sleep(500);
  const hash0 = page.debug().project.hash;
  const depth0 = page.debug().undoDepth;

  // Marquee over the whole clip, starting in the empty space left of the clip.
  const v = view(page);
  const [scrollY, rowH, scrollTick, pxPerTick] = v.v;
  assert(v.h >= RULER + 60 * rowH + VELOCITY_LANE, `the roll is ${v.h}px high, too short to show 60 pitch rows`);
  const from = { x: Math.round(v.l + KEYBOARD + 2), y: Math.round(v.t + RULER + 2) };
  const to = { x: Math.round(v.l + v.w - 4), y: Math.round(v.t + v.h - VELOCITY_LANE - 3) };
  // The driver cannot put Shift on a mouse event, so the Select tool stands in for Shift drag.
  page.click('Select');
  await sleep(200);
  dragTo(page, from, to, { steps: 8 });
  await sleep(500);
  out.status = status(page);
  assert(page.debug().project.selectedNotes === COUNT, `marquee should select ${COUNT} notes, got ${page.debug().project.selectedNotes} (${out.status})`);
  assert(/^200 selected \(/.test(out.status), `status line should name the keys, it says "${out.status}"`);
  assert(page.debug().project.hash === hash0 && page.debug().undoDepth === depth0, 'selecting must not edit the project');

  // Grab the first note and drag it two rows up: all 200 move as one step.
  const noteX = Math.round(v.l + KEYBOARD + (0 - scrollTick) * pxPerTick + 5);
  const noteY = Math.round(v.t + RULER + (127 - FIRST_PITCH) * rowH - scrollY + rowH / 2);
  page.run(['mouse', 'move', String(noteX), String(noteY)]);
  out.hoverClass = ev(page, `document.getElementById('piano-roll').className`);
  dragTo(page, { x: noteX, y: noteY }, { x: noteX, y: noteY - 2 * rowH }, { steps: 6 });
  await sleep(800);
  out.afterDrag = { depth: page.debug().undoDepth, depth0, status: status(page) };
  await waitDepth(page, depth0 + 1).catch((e) => {
    throw new Error(`${e.message} ${JSON.stringify(out.afterDrag)}`);
  });
  await sleep(300);
  const moved = page.debug();
  assert(moved.undoDepth === depth0 + 1, `the drag should be one undo step, depth went ${depth0} to ${moved.undoDepth}`);
  assert(moved.project.hash !== hash0, 'the drag should change the project');
  assert(moved.project.selectedNotes === COUNT, 'the selection should survive the move');
  out.moveLabel = moved.commandLog.at(-1);
  press(page, 'Control+z');
  await page.waitDebug((d) => d.undoDepth === depth0 && d.project.hash === hash0, 'undo back to the first hash');
  out.undoRestoredHash = true;

  // Note list: only the visible rows are in the page, and a typed loudness is one undo step.
  page.click('Note list');
  await sleep(500);
  await sleep(2500);
  const rows = ev(page, `document.querySelectorAll('[data-testid="note-row"]').length`);
  const dims = ev(page, `(() => { const e = document.querySelector('[data-testid="note-list-scroll"]'); const chain = []; for (let n = e; n && chain.length < 7; n = n.parentElement) chain.push(n.clientHeight); return [e.dataset.window, e.clientHeight, e.scrollHeight, e.scrollTop, getComputedStyle(e).paddingTop, getComputedStyle(e).paddingBottom, chain]; })()`);
  assert(rows > 3 && rows < 60, `the note list should window its ${COUNT} rows, found ${rows} in the page ${JSON.stringify(dims)}`);
  out.listRows = rows;
  const ref = page.findRef('Loudness of note 1');
  assert(ref, 'no loudness cell for the first note');
  page.run(['fill', ref, '33']);
  press(page, 'Enter');
  await waitDepth(page, depth0 + 1);
  assert(page.debug().project.hash !== hash0, 'a loudness edit should change the project');
  press(page, 'Control+z');
  await page.waitDebug((d) => d.undoDepth === depth0 && d.project.hash === hash0, 'undo of the list edit');
  out.listEditUndone = true;
  return out;
}
