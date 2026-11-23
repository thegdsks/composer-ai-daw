// U31: the library sidebar is open on first run, Shift+B collapses and reopens it, search narrows
// the list, a click auditions without editing, a double click uses the sound, one undo restores it.
import { assert } from './lib.mjs';

function state(page) {
  const out = page.run(['eval', "document.querySelector('[data-testid=library]')?.dataset.state ?? 'none'"]);
  return out.replace(/["\s]/g, '');
}

async function waitState(page, want) {
  const deadline = Date.now() + 5000;
  while (state(page) !== want) {
    assert(Date.now() < deadline, `library did not reach ${want}, last ${state(page)}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

export default async function library({ page, url }) {
  page.open(`${url}?e2e=1&start=off`);
  const start = await page.waitDebug((d) => d.project && d.project.tracks > 0, 'starter project');
  const depth0 = start.project.undoDepth;

  assert(state(page) === 'open', `first run should open the library, got ${state(page)}`);

  // Shift+B collapses to a rail and reopens.
  page.run(['press', 'Shift+B']);
  await waitState(page, 'rail');
  page.run(['press', 'Shift+B']);
  await waitState(page, 'open');

  // Search narrows the list.
  page.run(['fill', page.findRef('Search the library'), 'warm']);
  await page.waitDebug(
    () => page.run(['eval', "document.querySelectorAll('[data-testid=library-sound]').length"]).trim() === '2',
    'search filter',
  );

  // Click auditions and leaves the project alone.
  page.run(['click', '[data-sound=builtin-warm-pad]']);
  const heard = await page.waitDebug((d) => d.audio.previews >= 1, 'audition');
  assert(heard.audio.previewSound === 'builtin-warm-pad', `audition played ${heard.audio.previewSound}`);
  assert(heard.project.undoDepth === depth0, 'audition must not add an undo step');
  assert(heard.project.sounds[0] === null, 'audition must not change the track');

  // Double click uses it on the selected track, one undo step.
  page.run(['dblclick', '[data-sound=builtin-warm-pad]']);
  const used = await page.waitDebug((d) => d.project.sounds[0] === 'builtin-warm-pad', 'sound applied');
  assert(used.project.undoDepth === depth0 + 1, `expected one undo step, got ${used.project.undoDepth - depth0}`);

  page.click('Undo');
  const undone = await page.waitDebug((d) => d.project.sounds[0] === null, 'undo');
  assert(undone.project.undoDepth === depth0, `undo left depth ${undone.project.undoDepth}`);

  const errors = page.run(['errors'], { allowFail: true }).trim();
  assert(errors === '', `page errors: ${errors}`);

  return { previews: used.audio.previews, soundAfterUse: used.project.sounds[0], soundAfterUndo: undone.project.sounds[0] };
}
