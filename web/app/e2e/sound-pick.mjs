// U07: open the Sound tab, audition Warm Pad, pick it on track 1, see it in state, undo it in one step.
import { assert } from './lib.mjs';

// The dock is short in a small window, so bring a card into view before pressing its button.
function press(page, sound, verb) {
  // The library sidebar also has a [data-sound] row, so scroll the button itself, not the first card.
  const button = `[data-sound=${sound}] button[aria-label^=${verb}]`;
  page.run(['scrollintoview', button]);
  page.run(['click', button]);
}

export default async function soundPick({ page, url }) {
  page.open(`${url}?e2e=1&start=off`);
  const start = await page.waitDebug((d) => d.project && d.project.tracks > 0, 'starter project');
  const depth0 = start.project.undoDepth;
  assert(start.project.sounds[0] === null, 'track 1 should start without a catalogue sound');

  page.run(['click', '[data-testid=dock-tab-sound]']);
  await page.waitDebug(() => page.findRef('Hear Soft Piano') !== null, 'sound cards');
  assert(page.findRef('Search sounds'), 'search box missing');

  // Search narrows the list before any pick.
  page.run(['fill', page.findRef('Search sounds'), 'warm']);
  await page.waitDebug(() => page.findRef('Hear Soft Piano') === null && page.findRef('Hear Warm Strings') !== null, 'search filter');

  // Hear first: audition leaves the project alone.
  press(page, 'builtin-warm-pad', 'Hear');
  const heard = await page.waitDebug((d) => d.audio.previews >= 1, 'audition');
  assert(heard.audio.previewSound === 'builtin-warm-pad', `audition played ${heard.audio.previewSound}`);
  assert(heard.project.undoDepth === depth0, 'audition must not add an undo step');
  assert(heard.project.sounds[0] === null, 'audition must not change the track');

  press(page, 'builtin-warm-pad', 'Use');
  const picked = await page.waitDebug((d) => d.project.sounds[0] === 'builtin-warm-pad', 'sound applied');
  assert(picked.project.undoDepth === depth0 + 1, `expected one undo step, got ${picked.project.undoDepth - depth0}`);
  assert(picked.audio.previews >= 2, 'picking should play a short chord');

  // Undo reverts it in a single step.
  page.click('Undo');
  const undone = await page.waitDebug((d) => d.project.sounds[0] === null, 'undo');
  assert(undone.project.undoDepth === depth0, `undo left depth ${undone.project.undoDepth}`);

  return {
    previews: picked.audio.previews,
    soundAfterPick: picked.project.sounds[0],
    undoSteps: picked.project.undoDepth - depth0,
    soundAfterUndo: undone.project.sounds[0],
    audioState: picked.audio.state,
  };
}
