// U05 exit: move (across lanes), copy (Alt at release) and split (Alt click) each add exactly one
// undo step, and one undo restores the previous state hash. Run: node e2e/run.mjs region-edit
import { assert } from './lib.mjs';
import { boot, clipCenter, dragTo, press, sleep, state, waitDepth } from './clip-lib.mjs';

export default async function regionEdit({ page, url }) {
  await boot(page, url);
  const out = {};

  // Move: the Pad clip two bars later and down onto the Bass lane.
  let s = state(page);
  const pad = s.clips[0];
  const bass = s.clips[1];
  assert(pad.lane === 0 && bass.lane === 1, 'expected Pad on lane 0 and Bass on lane 1');
  const hash0 = s.hash;
  const depth0 = s.depth;
  const barPx = s.arr.pxPerTick * 3840;
  const from = clipCenter(pad);
  dragTo(page, from, { x: from.x + Math.round(barPx * 2), y: from.y + s.arr.rowHeight });
  await waitDepth(page, depth0 + 1);
  s = state(page);
  const moved = s.clips.find((c) => c.id === pad.id);
  assert(moved.lane === 1, `clip should be on lane 1, is on ${moved.lane}`);
  assert(moved.start === 3840 * 2, `clip should start at bar 3, starts at tick ${moved.start}`);
  out.move = { depth: s.depth, start: moved.start, lane: moved.lane };
  press(page, 'Control+z');
  await waitDepth(page, depth0);
  assert(state(page).hash === hash0, 'one undo did not restore the state hash after the move');

  // Copy: the same drag with Alt held at release leaves the original and adds one clip.
  await sleep(200);
  s = state(page);
  const before = s.clips.length;
  dragTo(page, from, { x: from.x + Math.round(barPx * 5), y: from.y }, { alt: true });
  await waitDepth(page, depth0 + 1);
  s = state(page);
  assert(s.clips.length === before + 1, `copy should add one clip, clips ${before} -> ${s.clips.length}: ${JSON.stringify(s.clips.map((c) => [c.lane, c.start, c.length]))} selected ${s.arr.selected.length}`);
  assert(s.clips.some((c) => c.id === pad.id && c.start === 0), 'the original clip must stay where it was');
  out.copy = { depth: s.depth, clips: s.clips.length };
  press(page, 'Control+z');
  await waitDepth(page, depth0);
  assert(state(page).hash === hash0, 'one undo did not restore the state hash after the copy');

  // Split: Alt click on the Pad clip cuts it at the snapped pointer, here the bar 3 line.
  await sleep(200);
  s = state(page);
  const split = { x: Math.round(pad.x + barPx * 2 + 4), y: from.y };
  page.run(['mouse', 'move', String(split.x), String(split.y)]);
  page.run(['keydown', 'Alt']);
  page.run(['mouse', 'down']);
  page.run(['mouse', 'up']);
  page.run(['keyup', 'Alt']);
  await waitDepth(page, depth0 + 1);
  s = state(page);
  const padParts = s.clips.filter((c) => c.lane === 0).sort((a, b) => a.start - b.start);
  assert(padParts.length === 2, `split should leave two Pad clips, found ${padParts.length}`);
  assert(padParts[1].start === 3840 * 2, `right part should start at bar 3, starts at ${padParts[1].start}`);
  out.split = { depth: s.depth, parts: padParts.map((c) => [c.start, c.length]) };
  press(page, 'Control+z');
  await waitDepth(page, depth0);
  assert(state(page).hash === hash0, 'one undo did not restore the state hash after the split');

  return out;
}
