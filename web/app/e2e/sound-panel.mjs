// U34: select track 2, change reverb send and volume in the Sound tab, each one undo step, undo restores.
// Also: next sound is one undo step, the master strip in the Mix tab edits the master reverb.
import { assert } from './lib.mjs';

const text = (page, testId) => {
  const out = page.run(['eval', `document.querySelector('${testId}')?.textContent ?? ''`]).trim();
  return out.replace(/^"|"$/g, '');
};
const valuetext = (page, label) => {
  const out = page.run(['eval', `document.querySelector('input[aria-label^="${label}"]')?.getAttribute('aria-valuetext') ?? ''`]).trim();
  return out.replace(/^"|"$/g, '');
};

export default async function soundPanel({ page, url }) {
  page.open(`${url}?e2e=1&start=off`);
  await page.waitDebug((d) => d.project && d.project.tracks >= 2, 'starter project with two tracks');

  // Select track 2 by pressing on its row (the name field sits inside it).
  page.run(['click', 'ul li:nth-of-type(2) input']);
  page.run(['click', '[data-testid=dock-tab-sound]']);
  await page.waitDebug(() => text(page, '[data-testid=sound-panel-detail]') !== '', 'sound panel');

  const depth0 = page.debug().undoDepth;

  // Reverb send: one arrow press is one commit.
  page.run(['focus', 'input[aria-label^="Reverb send of"]']);
  page.run(['press', 'ArrowRight']);
  await page.waitDebug((d) => d.undoDepth === depth0 + 1, 'reverb send command');
  const sent = valuetext(page, 'Reverb send of');
  assert(sent === '1 percent', `one reverb step should read 1 percent, got ${sent}`);

  // Volume: same rule.
  page.run(['focus', 'input[aria-label^="Volume of"]']);
  page.run(['press', 'ArrowLeft']);
  await page.waitDebug((d) => d.undoDepth === depth0 + 2, 'volume command');
  const vol = valuetext(page, 'Volume of');
  assert(vol === '99 percent', `one volume step should read 99 percent, got ${vol}`);

  // Next sound is one step too.
  page.run(['click', '[data-testid=sound-next]']);
  await page.waitDebug((d) => d.undoDepth === depth0 + 3, 'next sound');
  const named = text(page, '[data-testid=sound-panel-name]');
  assert(named !== 'Basic instrument', `next sound should name a sound, got ${named}`);

  // Undo three times restores everything.
  page.run(['focus', 'body']);
  for (let i = 0; i < 3; i++) page.run(['press', 'Control+z']);
  await page.waitDebug((d) => d.undoDepth === depth0, 'undo back to start');
  assert(valuetext(page, 'Reverb send of') === 'None', `reverb send after undo: ${valuetext(page, 'Reverb send of')}`);
  assert(valuetext(page, 'Volume of') === '100 percent', `volume after undo: ${valuetext(page, 'Volume of')}`);
  assert(text(page, '[data-testid=sound-panel-name]') === 'Basic instrument', 'sound should be back to the basic instrument');

  // Master strip in the Mix tab.
  page.run(['click', '[data-testid=dock-tab-mix]']);
  await page.waitDebug(() => valuetext(page, 'Reverb level') !== '', 'master strip');
  page.run(['focus', 'input[aria-label="Reverb level"]']);
  page.run(['press', 'ArrowRight']);
  await page.waitDebug((d) => d.undoDepth === depth0 + 1, 'master reverb command');
  assert(valuetext(page, 'Reverb level') === '1 percent', 'master reverb should read 1 percent');

  return { reverbSend: sent, volume: vol, soundAfterNext: named, undoSteps: 3 };
}
