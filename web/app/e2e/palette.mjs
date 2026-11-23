// U21: Ctrl K opens the palette, typing finds commands by word start, a disabled entry shows its
// reason, Enter runs one entry as one undo step, and a form entry asks only what it needs.
import { assert } from './lib.mjs';
import { ev, press, sleep, waitDepth } from './clip-lib.mjs';

const items = (page) => ev(page, `[...document.querySelectorAll('[data-testid=palette-item]')].map(l => ({ id: l.dataset.command, text: l.textContent, disabled: l.getAttribute('aria-disabled') }))`);

async function waitItems(page, pick, what) {
  const deadline = Date.now() + 5000;
  for (;;) {
    const list = items(page);
    if (pick(list)) return list;
    assert(Date.now() < deadline, `timed out waiting for ${what}: ${JSON.stringify(list)}`);
    await sleep(100);
  }
}

export default async function palette({ page, url }) {
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off`);
  const start = await page.waitDebug((d) => d.project && d.project.tracks > 0, 'starter project');
  const depth0 = start.undoDepth;
  const tracks0 = start.project.tracks;

  press(page, 'Control+k');
  const all = await waitItems(page, (l) => l.length > 30, 'the full command list');
  const withKey = ev(page, `document.querySelector('[data-testid=palette-dialog]').open`);
  assert(withKey === true, 'palette should be open');

  // A disabled entry says why, in words.
  page.run(['fill', page.findRef('Type what you want to do'), 'remove tempo']);
  const rt = await waitItems(page, (l) => l[0] && l[0].id === 'remove_tempo', 'Remove tempo change first');
  assert(rt[0].disabled === 'true', 'remove tempo should be disabled with one tempo');
  assert(/only one tempo/i.test(rt[0].text), `reason missing: ${rt[0].text}`);

  // Add track by words, run with Enter: one undo step.
  page.run(['fill', page.findRef('Type what you want to do'), 'add tr']);
  await waitItems(page, (l) => l[0] && l[0].id === 'add_track', 'Add track first');
  press(page, 'Enter');
  await waitDepth(page, depth0 + 1);
  const added = page.debug();
  assert(added.project.tracks === tracks0 + 1, `track count ${added.project.tracks}, expected ${tracks0 + 1}`);
  await page.waitDebug(() => ev(page, `document.querySelector('[data-testid=palette-dialog]').open === false`), 'palette closed after run');

  // A form entry: tempo asks for beat and tempo only, and runs as one step.
  press(page, 'Control+k');
  await waitItems(page, (l) => l.length > 30, 'list again');
  page.run(['fill', page.findRef('Type what you want to do'), 'change tempo']);
  await waitItems(page, (l) => l[0] && l[0].id === 'set_tempo', 'Change tempo first');
  press(page, 'Enter');
  const field = await (async () => {
    for (let i = 0; i < 30; i++) {
      const ref = page.findRef('Tempo (beats per minute)');
      if (ref) return ref;
      await sleep(100);
    }
    return null;
  })();
  assert(field, 'tempo form did not open');
  page.run(['fill', field, '77']);
  press(page, 'Enter');
  await waitDepth(page, depth0 + 2);
  const bpm = ev(page, `document.querySelector('input[aria-label="Tempo in beats per minute"]').value`);
  assert(String(bpm) === '77', `tempo field should read 77, got ${bpm}`);

  return { entries: all.length, tracksAfter: added.project.tracks, bpm };
}
