// U39 exit: a marker added by double click shows in the lane and one undo removes it; a tempo change
// at bar 5 by double click and a dialog is one undo step; tap tempo sets the tempo in one undo step;
// the position readout is there; a clip loop is set from the clip menu. Run: node e2e/run.mjs timeline-lanes
import { assert } from './lib.mjs';
import { boot, clipCenter, ev, press, sleep, state, waitDepth } from './clip-lib.mjs';

const BAR = 3840;
const tl = (page) => page.debug().project.timeline;

/** A real browser double click needs two quick presses, so the script sends the events itself. */
function dbl(page, sel, x, y) {
  const types = ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click', 'pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click', 'dblclick'];
  ev(page, `(()=>{const c=document.querySelector('${sel}');for(const t of ${JSON.stringify(types)})c.dispatchEvent(new MouseEvent(t,{bubbles:true,cancelable:true,clientX:${x},clientY:${y}}));return true})()`);
}

function lanePoint(page, lane, bar) {
  const { arr } = state(page);
  const r = ev(page, `(()=>{const b=document.getElementById('timeline-lanes-canvas').getBoundingClientRect();return {x:b.left,y:b.top,w:b.width,h:b.height}})()`);
  return { x: Math.round(r.x + (bar * BAR - arr.scrollTick) * arr.pxPerTick), y: Math.round(r.y + 12 + lane * 24), box: r };
}

export default async function timelineLanes({ page, url }) {
  await boot(page, url);
  const out = {};
  const depth0 = state(page).depth;

  // Lanes are hidden by default, the menu shows them.
  assert(ev(page, `document.getElementById('timeline-lanes-canvas') === null`), 'lanes must be hidden by default');
  for (const name of ['Show markers', 'Show tempo', 'Show key']) {
    page.click('Timeline lanes');
    await sleep(150);
    page.click(name);
    await sleep(150);
  }
  assert(ev(page, `document.getElementById('timeline-lanes-canvas') !== null`), 'lane canvas missing after showing the lanes');
  out.laneHeight = lanePoint(page, 0, 0).box.h;
  assert(out.laneHeight === 72, `three lanes should be 72 px tall, are ${out.laneHeight}`);

  // Position readout starts at the beginning.
  assert(ev(page, `document.querySelector('[data-testid="position-musical"]').textContent`) === '1.1.1', 'position readout should start at 1.1.1');

  // Marker: double click in the marker lane at bar 3.
  let p = lanePoint(page, 0, 2);
  dbl(page, '#timeline-lanes-canvas', p.x, p.y);
  await waitDepth(page, depth0 + 1);
  assert(tl(page).markers[String(2 * BAR)] === 'Marker 1', `marker at bar 3 missing: ${JSON.stringify(tl(page).markers)}`);
  out.marker = tl(page).markers;
  press(page, 'Control+z');
  await waitDepth(page, depth0);
  assert(Object.keys(tl(page).markers).length === 0, 'one undo must remove the marker');
  press(page, 'Control+Shift+z');
  await waitDepth(page, depth0 + 1);
  assert(tl(page).markers[String(2 * BAR)] === 'Marker 1', 'redo must bring the marker back');

  // Tempo change at bar 5: double click, type 90 in the dialog.
  p = lanePoint(page, 1, 4);
  dbl(page, '#timeline-lanes-canvas', p.x, p.y);
  await sleep(300);
  ev(page, `(()=>{const i=document.querySelector('[data-testid="lane-bpm"]');i.focus();const set=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;set.call(i,'90');i.dispatchEvent(new Event('input',{bubbles:true}));return true})()`);
  press(page, 'Enter');
  await waitDepth(page, depth0 + 2);
  assert(tl(page).tempos[String(4 * BAR)] === 90000, `tempo at bar 5 missing: ${JSON.stringify(tl(page).tempos)}`);
  out.tempo = tl(page).tempos;
  press(page, 'Control+z');
  await waitDepth(page, depth0 + 1);
  assert(tl(page).tempos[String(4 * BAR)] === undefined, 'one undo must remove the tempo change');

  // Tap tempo: four taps 400 ms apart (about 150 BPM) become one undo step.
  const before = tl(page).tempos['0'];
  const d1 = state(page).depth;
  ev(page, `(()=>{const b=document.querySelector('[data-testid="tap-tempo"]');[0,400,800,1200].forEach(t=>setTimeout(()=>b.click(),t));return true})()`);
  await waitDepth(page, d1 + 1);
  const tapped = tl(page).tempos['0'] / 1000;
  assert(tapped > 130 && tapped < 175, `tap tempo should land near 150 BPM, got ${tapped}`);
  assert(state(page).depth === d1 + 1, 'taps must make exactly one undo step');
  out.tap = { bpm: tapped, steps: state(page).depth - d1 };
  press(page, 'Control+z');
  await waitDepth(page, d1);
  assert(tl(page).tempos['0'] === before, 'one undo must restore the tempo');

  // Clip loop from the clip menu.
  const c = clipCenter(state(page).clips[0]);
  ev(page, `(()=>{const c=document.getElementById('arrangement-canvas');c.focus();c.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,clientX:${c.x},clientY:${c.y}}));return true})()`);
  await sleep(250);
  page.click('Loop length: Off');
  await sleep(300);
  ev(page, `(()=>{const s=document.querySelector('[data-testid="clip-loop-select"]');const o=[...s.options].find(o=>o.value==='${BAR}');if(!o)return false;s.value=o.value;s.dispatchEvent(new Event('change',{bubbles:true}));return true})()`);
  press(page, 'Enter');
  await waitDepth(page, d1 + 1);
  assert(tl(page).loops === 1, `clip loop missing: ${JSON.stringify(tl(page))}`);
  out.loop = tl(page).loops;
  press(page, 'Control+z');
  await waitDepth(page, d1);
  assert(tl(page).loops === 0, 'one undo must clear the clip loop');
  return out;
}
