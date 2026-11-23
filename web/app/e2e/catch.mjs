// U36 exit: play 4 keys without arming Record, click Catch, a clip with 4 notes appears in one
// undo step, and one Undo removes it. Also checks the on screen key lights and plays.
// Run: E2E_PORT=<port> E2E_SESSION=<name> node e2e/run.mjs catch
import { assert } from './lib.mjs';
import { announcer, boot, ev, press, sleep } from './clip-lib.mjs';

const caught = (page) => page.debug().project.caught;

export default async function catchE2e({ page, url }) {
  await boot(page, url);
  const out = {};
  const before = page.debug().project;
  const depth0 = page.debug().undoDepth;

  // The Keys tab turns the computer keys on; no Record is armed.
  page.run(['click', '[data-testid=dock-tab-keys]']);
  await page.waitDebug(() => ev(page, `document.querySelector('[data-testid="keys-panel"]') !== null`), 'keys panel');
  assert(page.debug().project.record.phase === 'idle', 'Record must stay idle');
  assert(ev(page, `document.querySelectorAll('[data-testid="screen-keys"] button[data-pitch]').length`) === 25, 'two octaves and the top C are 25 keys');
  assert(ev(page, `document.querySelector('[data-testid="catch"]').dataset.count`) === '0', 'Catch should start empty');

  // Four computer key notes, a beat apart.
  for (const key of ['a', 's', 'd', 'f']) {
    press(page, key);
    await sleep(250);
  }
  await page.waitDebug((d) => d.project.caught === 4, 'four caught notes');
  out.buffered = caught(page);

  // Octave shift: the label follows.
  page.run(['click', '[data-testid=octave-up]']);
  assert(ev(page, `document.querySelector('[data-testid="octave-label"]').textContent`).includes('C5'), 'octave up should start the keys at C5');
  page.run(['click', '[data-testid=octave-down]']);

  // Catch: one group, one clip, four notes.
  page.run(['click', '[data-testid=catch]']);
  const after = await page.waitDebug((d) => d.project.notes === before.notes + 4, `${before.notes + 4} notes after Catch`);
  assert(after.project.regions === before.regions + 1, `one new clip expected, regions ${before.regions} then ${after.project.regions}`);
  assert(after.undoDepth === depth0 + 1, `Catch should be one undo step: depth ${depth0} then ${after.undoDepth}`);
  assert(after.project.caught === 0, 'the buffer should be empty after Catch');
  await announcer(page, (t) => /Catch 4 notes/.test(t), '"Catch 4 notes"');
  out.catch = { clips: after.project.regions - before.regions, notes: after.project.notes - before.notes, steps: after.undoDepth - depth0 };

  // One Undo takes the clip back.
  press(page, 'Control+z');
  await page.waitDebug((d) => d.project.notes === before.notes && d.project.regions === before.regions, 'the catch undone');
  assert(page.debug().undoDepth === depth0, 'undo should return to the depth before Catch');
  out.undone = true;
  return out;
}
