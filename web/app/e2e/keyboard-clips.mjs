// U05 exit: Tab to the arrangement, arrows reach clip 2, Enter opens it in Notes, the live region
// reads its bars. Keyboard only after the page loads. Run: node e2e/run.mjs keyboard-clips
import { assert } from './lib.mjs';
import { announcer, boot, ev, press, state } from './clip-lib.mjs';

const activeLabel = `(document.activeElement && document.activeElement.getAttribute('aria-label')) || ''`;

export default async function keyboardClips({ page, url }) {
  await boot(page, url);
  const out = {};

  // Tab until the arrangement canvas has focus.
  let tabs = 0;
  for (; tabs < 60; tabs++) {
    press(page, 'Tab');
    if (String(ev(page, activeLabel)).startsWith('Arrangement.')) break;
  }
  assert(tabs < 60, 'Tab never reached the arrangement canvas');
  out.tabs = tabs + 1;

  // Clip 2 of the Pad lane: N adds a clip after the first one, then walk back and forth.
  const first = state(page).clips.filter((c) => c.lane === 0)[0];
  press(page, 'n');
  await announcer(page, (t) => /Pad, bars 5 to 8/.test(t), '"Pad, bars 5 to 8" after N');
  press(page, 'ArrowLeft');
  const left = await announcer(page, (t) => /Pad, bars 1 to 4/.test(t), '"Pad, bars 1 to 4"');
  assert(state(page).arr.selected[0] === first.id, 'ArrowLeft should select the first Pad clip');
  press(page, 'ArrowRight');
  const right = await announcer(page, (t) => /Pad, bars 5 to 8/.test(t), '"Pad, bars 5 to 8" after ArrowRight');
  const second = state(page).clips.filter((c) => c.lane === 0 && c.id !== first.id)[0];
  assert(state(page).arr.selected[0] === second.id, 'ArrowRight should select clip 2');
  out.read = { left, right };

  // Down and up cross lanes; Enter opens the clip in the Notes tab and focuses the roll.
  press(page, 'ArrowDown');
  await announcer(page, (t) => /^Bass, /.test(t), 'a Bass clip after ArrowDown');
  press(page, 'ArrowUp');
  await announcer(page, (t) => /^Pad, /.test(t), 'a Pad clip after ArrowUp');
  press(page, 'Enter');
  await announcer(page, (t) => /Editing Pad in Notes/.test(t), '"Editing Pad in Notes" after Enter');
  const tab = ev(page, `document.querySelector('[data-testid="dock-tab-notes"]').getAttribute('aria-selected')`);
  assert(tab === 'true', 'Enter should show the Notes tab');
  const focused = ev(page, `document.activeElement && document.activeElement.id`);
  assert(focused === 'piano-roll', `the piano roll should have focus, "${focused}" has`);
  out.opened = { notesTab: tab, focus: focused };
  return out;
}
