// U04 exit: on a fresh profile the start sheet has exactly one primary button, one click on a goal card
// plays a template by itself (one action, one undo step), and Undo brings the starter back.
import { assert, envNum } from './lib.mjs';
import { ev } from './clip-lib.mjs';

export default async function startCinematic({ page, url }) {
  const budget = envNum('VITE_E2E_FIRST_SOUND_MS', 10000);
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1`);
  await page.waitDebug((d) => d.project && d.project.tracks > 0, 'starter project');
  assert(page.findRef('Cinematic idea'), 'goal card Cinematic idea missing');
  const primaries = ev(page, `[...document.querySelectorAll('[data-primary]')].map((e) => e.textContent.trim())`);
  assert(primaries.length === 1, `expected one data-primary on the start screen, got ${JSON.stringify(primaries)}`);
  assert(primaries[0] === 'Play the starter', `primary is "${primaries[0]}"`);
  const before = page.debug();
  assert(before.firstSoundMs === null, 'no sound may play before the first click');
  assert(before.undoDepth === 0, `starter should open with no undo steps, got ${before.undoDepth}`);

  page.click('Cinematic idea');
  const d = await page.waitDebug((x) => x.firstSoundMs !== null, 'first sound', budget);
  assert(d.firstSoundMs <= budget, `firstSoundMs ${d.firstSoundMs} over budget ${budget}`);
  assert(d.actionsToFirstSound === 1, `actionsToFirstSound is ${d.actionsToFirstSound}, expected 1`);
  assert(d.audio.state !== 'error', `audio error: ${d.audio.error}`);
  const after = await page.waitDebug((x) => x.project.tracks === 4, 'template applied');
  assert(after.undoDepth === 1, `undoDepth is ${after.undoDepth}, expected 1`);
  assert(after.project.sounds.every((s) => typeof s === 'string' && s.startsWith('builtin-')), `sounds ${JSON.stringify(after.project.sounds)}`);
  const hash = ev(page, 'location.hash');
  assert(/^#\/p\/[0-9a-f]+$/.test(hash), `route is ${hash}`);
  assert(ev(page, `document.querySelector('[data-testid=start-sheet]') === null`), 'start sheet should be gone');
  const primariesAfter = ev(page, `document.querySelectorAll('[data-primary]').length`);
  assert(primariesAfter <= 1, `${primariesAfter} data-primary elements on the workspace`);

  page.click('Undo');
  const undone = await page.waitDebug((x) => x.project.tracks === 2, 'undo');
  assert(undone.undoDepth === 0, `undo left depth ${undone.undoDepth}`);

  return {
    firstSoundMs: Math.round(d.firstSoundMs),
    gestureToSoundMs: Math.round(d.firstSoundMs - d.firstGestureMs),
    actionsToFirstSound: d.actionsToFirstSound,
    undoDepth: after.undoDepth,
    tracks: after.project.tracks,
    sounds: after.project.sounds,
    route: hash,
    undoRestoresTracks: undone.project.tracks,
  };
}
