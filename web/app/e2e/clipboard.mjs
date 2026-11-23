// U11 exit: copy the notes of bar 1, paste at bar 5 and they land 4 bars later; repeat a clip 4 times is
// one undo step. Keyboard only. Run: node e2e/run.mjs clipboard
import { assert } from './lib.mjs';
import { announcer, boot, ev, press, sleep, waitDepth } from './clip-lib.mjs';

const BAR = 3840;
const activeLabel = `(document.activeElement && document.activeElement.getAttribute('aria-label')) || ''`;

async function toArrangement(page) {
  for (let tabs = 0; tabs < 80; tabs++) {
    press(page, 'Tab');
    if (String(ev(page, activeLabel)).startsWith('Arrangement.')) return;
  }
  throw new Error('Tab never reached the arrangement canvas');
}

export default async function clipboard({ page, url }) {
  await boot(page, url);
  const out = {};
  await toArrangement(page);

  // Playhead to bar 5 (four bar steps), then open the first clip in Notes.
  for (let i = 0; i < 4; i++) press(page, 'Period');
  await sleep(300);
  press(page, 'Enter');
  await announcer(page, (t) => /Editing Pad in Notes/.test(t), '"Editing Pad in Notes"');
  const before = page.debug().project.openNoteStarts;
  assert(before.length > 0, 'the Pad clip should have notes');

  // Copy every note, paste at the playhead (bar 5).
  press(page, 'Control+a');
  press(page, 'Control+c');
  await announcer(page, (t) => /^Copied \d+ notes?/.test(t), '"Copied n notes"');
  const depth = page.debug().undoDepth;
  press(page, 'Control+v');
  await waitDepth(page, depth + 1);
  const said = await announcer(page, (t) => /^Paste \d+ notes?/.test(t), '"Paste n notes"');
  const after = page.debug().project.openNoteStarts;
  assert(after.length === before.length * 2, `expected ${before.length * 2} notes, found ${after.length}`);
  const expected = [...before, ...before.map((s) => s + 4 * BAR)].sort((a, b) => a - b);
  assert(JSON.stringify(after) === JSON.stringify(expected), `pasted notes are not 4 bars later: ${JSON.stringify(after)}`);
  out.paste = { said, notes: after.length };

  // Undo takes the paste back in one step.
  press(page, 'Control+z');
  await waitDepth(page, depth);
  assert(page.debug().project.openNoteStarts.length === before.length, 'undo should remove all pasted notes');

  // Repeat the clip 4 times: Ctrl Shift D, type 4, Enter. One undo step.
  await toArrangement(page);
  const regions = page.debug().project.regions;
  const d2 = page.debug().undoDepth;
  press(page, 'Control+Shift+d');
  await sleep(300);
  const dialog = ev(page, `!!document.querySelector('dialog[open] #repeat-title')`);
  assert(dialog, 'Ctrl Shift D should open the Repeat dialog');
  const field = page.findRef('How many more times');
  assert(field, 'no count field in the Repeat dialog');
  page.run(['fill', field, '4']);
  press(page, 'Enter');
  await waitDepth(page, d2 + 1);
  assert(page.debug().project.regions === regions + 4, `repeat 4 times should add 4 clips (${regions} before, ${page.debug().project.regions} after)`);
  const said2 = await announcer(page, (t) => /Repeat 4 times/.test(t), '"Repeat 4 times"');
  press(page, 'Control+z');
  await waitDepth(page, d2);
  assert(page.debug().project.regions === regions, 'one undo should remove all 4 repeats');
  out.repeat = { said: said2, regions: regions + 4 };
  return out;
}
