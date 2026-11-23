// U14 exit: arm Record, wait out the count in, send 4 synthetic key presses, stop; the clip gains
// 4 notes in one undo step, Keys is off again afterwards, and one Undo takes the take back.
// Run: node e2e/run.mjs record
import { assert } from './lib.mjs';
import { announcer, boot, ev, press, sleep } from './clip-lib.mjs';

const phase = (page) => ev(page, `document.querySelector('[data-testid="record"]').dataset.phase`);
const audio = (page) => page.debug().audio;

async function waitPhase(page, want, timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const p = phase(page);
    if (p === want) return;
    assert(Date.now() < deadline, `timed out waiting for the Record button phase "${want}" (now "${p}")`);
    await sleep(80);
  }
}

export default async function record({ page, url }) {
  await boot(page, url);
  const out = {};
  const before = page.debug();
  const notes0 = before.project.notes;
  const depth0 = before.undoDepth;

  // Keys start off: a piano key does nothing until Keys or Record is on.
  assert(ev(page, `document.querySelector('[data-testid="keys-toggle"]').getAttribute('aria-pressed')`) === 'false', 'Keys should start off');
  out.keysStartOff = true;

  // 1. Arm with the button: the count in plays first, then recording starts.
  page.click('Record');
  await waitPhase(page, 'counting');
  const peakDuring = audio(page);
  assert(peakDuring.transport === 'playing' && peakDuring.counting, 'count in should be playing');
  // A press during the count in is not recorded.
  press(page, 'g');
  await waitPhase(page, 'recording', 10000);
  out.countedIn = true;

  // 2. Four presses, one beat apart (84 bpm is about 0.71 s).
  for (const key of ['a', 's', 'd', 'f']) {
    await sleep(650);
    press(page, key);
  }
  await sleep(300);

  // 3. R stops. One group lands: depth plus one, four more notes.
  press(page, 'r');
  await waitPhase(page, 'idle');
  const after = await page.waitDebug((d) => d.project.notes === notes0 + 4, `${notes0 + 4} notes after the take`);
  assert(after.undoDepth === depth0 + 1, `take should be one undo step: depth ${depth0} then ${after.undoDepth}`);
  await announcer(page, (t) => /Record 4 notes/.test(t), '"Record 4 notes"');
  out.take = { notes: after.project.notes - notes0, steps: after.undoDepth - depth0 };
  // Tidy timing is on at eighth notes (480 ticks): starts are on the grid, in order, and start near the clip start.
  const starts = [...after.project.record.lastStarts].sort((x, y) => x - y);
  assert(starts.length === 4 && starts.every((t) => t % 480 === 0), `starts should be four grid ticks, got ${JSON.stringify(starts)}`);
  assert(starts[0] <= 3840, `the first press came ${starts[0]} ticks into the clip, expected within a bar`);
  assert(starts.every((t, i) => i === 0 || t - starts[i - 1] >= 480), `starts should be at least one eighth apart: ${JSON.stringify(starts)}`);
  out.starts = starts;

  // 4. Keys went off with the recording, so a key press adds nothing and the roll keeps its shortcuts.
  assert(ev(page, `document.querySelector('[data-testid="keys-toggle"]').getAttribute('aria-pressed')`) === 'false', 'Keys should be off after recording');

  // 5. One Undo takes the whole take back.
  press(page, 'Control+z');
  await page.waitDebug((d) => d.project.notes === notes0, 'the take undone');
  assert(page.debug().undoDepth === depth0, 'undo should return to the depth before the take');
  out.undone = true;
  return out;
}
