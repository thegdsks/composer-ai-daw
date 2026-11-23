// Mixer exit check: mute track 2 in the Mix tab while playing, its meter reads 0, undo unmutes.
// Also drives a fader from the keyboard and checks one command per release and Home reset.
import { assert, envNum } from './lib.mjs';

const read = (page, js) => {
  const out = page.run(['eval', js]).trim();
  let v = out;
  for (let i = 0; i < 2 && typeof v === 'string'; i++) {
    try {
      v = JSON.parse(v);
    } catch {
      break;
    }
  }
  return v;
};

const levels = (page) =>
  read(page, "JSON.stringify([0,1].map(i => Number(document.querySelector('[data-testid=meter-' + i + ']')?.dataset.level ?? -1)))");

/** Samples both meters for windowMs and returns the highest level seen per track. */
async function peakOver(page, windowMs) {
  const end = Date.now() + windowMs;
  const best = [0, 0];
  while (Date.now() < end) {
    const l = levels(page);
    best[0] = Math.max(best[0], l[0]);
    best[1] = Math.max(best[1], l[1]);
    await new Promise((r) => setTimeout(r, 120));
  }
  return best;
}

export default async function mixer({ page, url }) {
  const window = envNum('E2E_METER_WINDOW_MS', 2500);
  // The strip buttons sit at the dock bottom: use the 1280x800 the other scripts use, the driver default is shorter.
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off`);
  await page.waitDebug((d) => d.project && d.project.tracks >= 2, 'starter project with two tracks');
  page.click('Mix');
  await page.waitDebug(() => read(page, "String(!!document.querySelector('[data-testid=strip-1]'))") === 'true' || read(page, "!!document.querySelector('[data-testid=strip-1]')") === true, 'mixer strips');
  page.click('Play (Space)');
  await page.waitDebug((d) => d.audio.transport === 'playing' && d.audio.positionFrames > 0, 'playing');

  const before = await peakOver(page, window);
  assert(before[0] > 0 && before[1] > 0, `both meters should move while playing, saw ${before}`);

  page.run(['click', '[data-testid=strip-mute-1]']);
  await page.waitDebug((d) => d.undoDepth >= 1, 'mute command applied');
  await new Promise((r) => setTimeout(r, 600));
  const muted = await peakOver(page, window);
  assert(muted[1] === 0, `track 2 meter should read 0 while muted, saw ${muted[1]}`);
  assert(muted[0] > 0, 'track 1 meter should keep moving');
  const pressed = read(page, "document.querySelector('[data-testid=strip-mute-1]').getAttribute('aria-pressed')");
  assert(String(pressed) === 'true', `mute button should be pressed, got ${pressed}`);

  page.run(['focus', 'body']);
  page.run(['press', 'Control+z']);
  await new Promise((r) => setTimeout(r, 600));
  const after = await peakOver(page, window);
  assert(after[1] > 0, 'undo should unmute track 2 so its meter moves again');
  const unpressed = read(page, "document.querySelector('[data-testid=strip-mute-1]').getAttribute('aria-pressed')");
  assert(String(unpressed) === 'false', `mute button should be released after undo, got ${unpressed}`);

  // Keyboard fader: arrow keys draft locally, one command on key release, Home resets to 0 dB.
  const depth0 = page.debug().undoDepth;
  page.run(['focus', '[data-testid=fader-0]']);
  for (let i = 0; i < 4; i++) page.run(['press', 'ArrowDown']);
  await page.waitDebug((d) => d.undoDepth >= depth0 + 4, 'fader command');
  const text = read(page, "document.querySelector('[data-testid=fader-0]').getAttribute('aria-valuetext')");
  assert(/^-2\.0 dB$/.test(text), `four arrow presses of 0.5 dB should read -2.0 dB, got ${text}`);
  page.run(['press', 'Home']);
  await page.waitDebug((d) => d.undoDepth >= depth0 + 5, 'reset command');
  const reset = read(page, "document.querySelector('[data-testid=fader-0]').getAttribute('aria-valuetext')");
  assert(/^0\.0 dB$/.test(reset), `Home should reset to 0.0 dB, got ${reset}`);

  return { before, muted, after, faderText: text, resetText: reset };
}
