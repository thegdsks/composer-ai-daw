// U13 exit: set A minor, draw on an out of key row with Stay in key on, the note lands on a neighbour in key.
// Run: node e2e/run.mjs roll-key
import { assert } from './lib.mjs';
import { boot, dragTo, ev, press, sleep, waitDepth } from './clip-lib.mjs';

const ROW_TOP = 28; // VITE_RULER_HEIGHT
const KEYBOARD = 52; // VITE_KEYBOARD_WIDTH
const VELOCITY_LANE = 60; // VITE_VELOCITY_LANE, the bottom strip of the canvas is not the grid
const NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
const name = (p) => `${NAMES[p % 12]}${Math.floor(p / 12) - 1}`;

const activeLabel = `(document.activeElement && document.activeElement.getAttribute('aria-label')) || ''`;

function view(page) {
  return ev(
    page,
    `(() => { const c = document.getElementById('piano-roll'); const r = c.getBoundingClientRect(); return { l: r.left, t: r.top, h: r.height, v: c.dataset.view.split(',').map(Number) }; })()`,
  );
}

const status = (page) => ev(page, `document.querySelector('[role="status"]').textContent`);

export default async function rollKey({ page, url }) {
  await boot(page, url);
  const out = {};

  // Open the first Pad clip in Notes with the keyboard.
  let tabs = 0;
  for (; tabs < 60; tabs++) {
    press(page, 'Tab');
    if (String(ev(page, activeLabel)).startsWith('Arrangement.')) break;
  }
  assert(tabs < 60, 'Tab never reached the arrangement canvas');
  press(page, 'Enter');
  await sleep(400);

  // A minor from the key picker.
  const root = page.findRef('Key note');
  assert(root, 'no key picker in the top bar');
  page.run(['select', root, '9']);
  await sleep(300);
  const scale = page.findRef('Scale');
  assert(scale, 'no scale picker');
  page.run(['select', scale, 'minor']);
  await sleep(300);
  const stay = ev(page, `document.querySelector('[data-testid="stay-in-key"]').textContent`);
  assert(stay === 'Stay in key: on', `Stay in key should be on, it says "${stay}"`);
  out.stay = stay;

  // Draw on the G# row (octave 3), in empty space past the starter notes, inside the clip.
  const before = page.debug().project.notes;
  const depth = page.debug().undoDepth;
  const v = view(page);
  const [scrollY, rowH, scrollTick, pxPerTick] = v.v;
  // The grid is short (ruler on top, velocity lane below), so aim at an out of key row that is fully inside it.
  const gridBottom = v.h - VELOCITY_LANE;
  const centre = (p) => ROW_TOP + (127 - p) * rowH - scrollY + rowH / 2;
  let pitch = -1;
  for (let p = 127; p >= 0; p--) {
    const c = centre(p);
    if ([1, 3, 6, 8, 10].includes(p % 12) && c - rowH / 2 >= ROW_TOP && c + rowH / 2 <= gridBottom) {
      pitch = p;
      break;
    }
  }
  assert(pitch >= 0, 'no out of key row is fully on screen; the roll did not scroll to the clip notes');
  const y = Math.round(v.t + centre(pitch));
  const x = Math.round(v.l + KEYBOARD + (3000 - scrollTick) * pxPerTick);
  dragTo(page, { x, y }, { x: x + 40, y }, { steps: 3 });
  await waitDepth(page, depth + 1);
  assert(page.debug().project.notes === before + 1, 'drawing should add one note');
  const text = status(page);
  const m = text.match(/\((\w#?\d)\)/);
  assert(m, `the status line should name the note, it says "${text}"`);
  assert([name(pitch - 1), name(pitch + 1)].includes(m[1]), `${name(pitch)} should land on ${name(pitch - 1)} or ${name(pitch + 1)}, it landed on ${m[1]}`);
  out.note = m[1];

  // Stay in key off: the same row keeps G#.
  page.run(['click', page.findRef('Stay in key')]);
  await sleep(200);
  const y2 = y;
  const x2 = x + 400;
  dragTo(page, { x: x2, y: y2 }, { x: x2 + 40, y: y2 }, { steps: 3 });
  await waitDepth(page, depth + 2);
  const m2 = status(page).match(/\((\w#?\d)\)/);
  assert(m2 && m2[1] === name(pitch), `with Stay in key off the note should be ${name(pitch)}, got ${m2 && m2[1]}`);
  out.off = m2[1];
  return out;
}
