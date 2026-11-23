// U19 exit: in C major click Am at bar 2, the clip gains A, C, E at bar 2 in one undo step.
// Also checks collapse is remembered. Run: node e2e/run.mjs chord
import { assert } from './lib.mjs';
import { announcer, boot, ev, press, sleep, waitDepth } from './clip-lib.mjs';

const BAR = 3840;
const activeLabel = `(document.activeElement && document.activeElement.getAttribute('aria-label')) || ''`;

export default async function chord({ page, url }) {
  await boot(page, url);
  const out = {};

  let tabs = 0;
  for (; tabs < 60; tabs++) {
    press(page, 'Tab');
    if (String(ev(page, activeLabel)).startsWith('Arrangement.')) break;
  }
  assert(tabs < 60, 'Tab never reached the arrangement canvas');
  press(page, 'Period'); // playhead to bar 2
  await sleep(300);
  press(page, 'Enter');
  await sleep(400);

  // The strip starts folded so the roll keeps its height; open it from the Chords button.
  assert(!ev(page, `!!document.querySelector('[data-testid="chord-strip"]')`), 'the strip should start folded');
  page.run(['click', page.findRef('Chords')]);
  await sleep(300);

  // Before a key exists the strip says so.
  const empty = ev(page, `!!document.querySelector('[data-testid="chord-strip-empty"]')`);
  out.emptyWithoutKey = empty;

  const root = page.findRef('Key note');
  assert(root, 'no key picker in the top bar');
  page.run(['select', root, '0']);
  await sleep(300);
  const scale = page.findRef('Scale');
  page.run(['select', scale, 'major']);
  await sleep(500);

  const names = ev(page, `[...document.querySelectorAll('[data-chord]')].map((b) => b.dataset.chord)`);
  assert(JSON.stringify(names) === JSON.stringify(['C', 'Dm', 'Em', 'F', 'G', 'Am', 'Bdim']), `chord names: ${JSON.stringify(names)}`);
  out.names = names;

  const before = page.debug().project.openNotes.length;
  const depth = page.debug().undoDepth;
  const am = page.findRef('Am chord');
  assert(am, 'no Am button');
  page.run(['click', am]);
  await waitDepth(page, depth + 1);
  const said = await announcer(page, (t) => /^Chord Am in bar 2/.test(t), '"Chord Am in bar 2"');
  const notes = page.debug().project.openNotes;
  const atBar2 = notes.filter((n) => n.start >= BAR && n.start < 2 * BAR).map((n) => n.pitch % 12).sort((a, b) => a - b);
  assert(JSON.stringify(atBar2) === JSON.stringify([0, 4, 9]), `bar 2 should hold C, E, A, found ${JSON.stringify(atBar2)} (${notes.length} notes, was ${before})`);
  out.bar2 = atBar2;
  out.said = said;

  press(page, 'Control+z');
  await waitDepth(page, depth);
  assert(page.debug().project.openNotes.length === before, 'one undo should restore the clip');

  // Fold it again; the choice is remembered in prefs.
  page.run(['click', page.findRef('Chords')]);
  await sleep(200);
  assert(!ev(page, `!!document.querySelector('[data-testid="chord-strip"]')`), 'the strip should fold away');
  assert(ev(page, `JSON.parse(localStorage.getItem('composer.prefs.v1') ?? '{}').chordStrip === false`), 'the folded state should be in prefs');
  out.collapsed = true;
  return out;
}
