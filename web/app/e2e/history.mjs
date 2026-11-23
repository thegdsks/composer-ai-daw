// U21: make 5 edits, open the history panel by key, click step 2, and the state hash equals the
// hash recorded after edit 2. Then jump forward again, and use the keyboard inside the list.
import { assert } from './lib.mjs';
import { announcer, ev, press, sleep, waitDepth } from './clip-lib.mjs';

const EDITS = 5;

export default async function history({ page, url }) {
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off`);
  const start = await page.waitDebug((d) => d.project && d.project.tracks > 0, 'starter project');
  const depth0 = start.undoDepth;
  const hashes = [start.project.hash];

  // Add track is one undo step each time, and it changes the hash every time.
  for (let i = 0; i < EDITS; i++) {
    page.click('Add track');
    await waitDepth(page, depth0 + i + 1);
    hashes.push(page.debug().project.hash);
  }
  assert(new Set(hashes).size === hashes.length, 'every edit should change the hash');

  // Ctrl Alt Z opens the panel; the keyboard shortcut is the path under test here.
  page.run(['press', 'Control+Alt+z']);
  await page.waitDebug(() => ev(page, `document.querySelector('[data-testid=history-panel]') !== null`), 'history panel');
  const rows = ev(page, `[...document.querySelectorAll('[data-testid=history-step]')].map(b => b.textContent)`);
  assert(rows.length === 6, `expected the opening state and 5 steps, saw ${rows.length}: ${JSON.stringify(rows)}`);
  const here = ev(page, `document.querySelector('[data-testid=history-step][aria-current=step]').dataset.step`);
  assert(String(here) === '5', `current step should be 5, got ${here}`);
  assert(rows[1].includes('You'), `step 1 should name its source: ${rows[1]}`);

  // Click step 2 by its button.
  page.run(['eval', `document.querySelector('[data-testid=history-step][data-step="2"]').click()`]);
  await waitDepth(page, 2);
  const after = page.debug().project.hash;
  assert(after === hashes[2], `hash after clicking step 2 (${after}) should equal the hash after edit 2 (${hashes[2]})`);
  const said = await announcer(page, (t) => t.startsWith('Went back to'), 'Went back to');

  // Redo forward to step 4 in one call, then back to the opening state.
  page.run(['eval', `document.querySelector('[data-testid=history-step][data-step="4"]').click()`]);
  await waitDepth(page, 4);
  assert(page.debug().project.hash === hashes[4], 'jump forward to step 4 should restore its hash');
  page.run(['eval', `document.querySelector('[data-testid=history-step][data-step="0"]').click()`]);
  await waitDepth(page, 0);
  assert(page.debug().project.hash === hashes[0], 'step 0 should restore the opening hash');

  // Keyboard: focus sits on the current row; arrow down then Enter moves one step.
  page.run(['eval', `document.querySelector('[data-testid=history-step][aria-current=step]').focus()`]);
  press(page, 'ArrowDown');
  await sleep(150);
  press(page, 'Enter');
  await waitDepth(page, 1);
  assert(page.debug().project.hash === hashes[1], 'ArrowDown then Enter should go to step 1');

  // Escape closes the panel and returns the focus.
  press(page, 'Escape');
  await page.waitDebug(() => ev(page, `document.querySelector('[data-testid=history-panel]') === null`), 'panel closed');

  return { edits: EDITS, hashAfterStep2: hashes[2], announced: said };
}
