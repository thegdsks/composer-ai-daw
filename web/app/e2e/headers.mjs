// U35: track header v2. Rename by double click (Enter commits, Escape cancels), duplicate track in one undo step,
// reorder by keyboard on the grip (one undo step).
// Run: E2E_PORT=<free port> E2E_SESSION=<name> node e2e/run.mjs headers
import { assert } from './lib.mjs';
import { boot, ev, press, sleep } from './clip-lib.mjs';

const NAMES = `[...document.querySelectorAll('[data-track-id] [data-testid=track-name]')].map((b) => b.textContent)`;
const MENU_OPEN = `Boolean(document.querySelector('[data-testid=context-menu]:popover-open'))`;

async function until(page, expr, what) {
  const deadline = Date.now() + 8000;
  while (!ev(page, expr)) {
    assert(Date.now() < deadline, `timed out waiting for ${what}`);
    await sleep(100);
  }
}

export default async function headers({ page, url }) {
  await boot(page, url);
  const start = page.debug();
  const out = {};
  const names0 = ev(page, NAMES);
  assert(names0.length >= 2, `need two tracks, saw ${names0}`);

  // Rename by double click. Escape cancels, Enter commits.
  page.run(['dblclick', '[data-track-id] [data-testid=track-name]']);
  await until(page, `Boolean(document.querySelector('[data-testid=track-name-input]'))`, 'the rename box');
  page.run(['fill', '[data-testid=track-name-input]', 'Dropped']);
  press(page, 'Escape');
  await until(page, `!document.querySelector('[data-testid=track-name-input]')`, 'Escape to close the box');
  assert(JSON.stringify(ev(page, NAMES)) === JSON.stringify(names0), 'Escape should keep the old name');
  assert(page.debug().project.undoDepth === start.project.undoDepth, 'Escape should not add a history step');

  page.run(['dblclick', '[data-track-id] [data-testid=track-name]']);
  await until(page, `Boolean(document.querySelector('[data-testid=track-name-input]'))`, 'the rename box again');
  page.run(['fill', '[data-testid=track-name-input]', 'Lead line']);
  press(page, 'Enter');
  const renamed = await page.waitDebug((d) => d.project.undoDepth === start.project.undoDepth + 1, 'the rename step');
  assert(ev(page, NAMES)[0] === 'Lead line', `name after Enter: ${ev(page, NAMES)}`);
  out.rename = ev(page, NAMES)[0];

  // Duplicate track from the header menu: one undo step, clips copied.
  const clipsBefore = renamed.project.regions;
  page.run(['click', '[data-track-id] [data-testid=track-menu]']);
  await until(page, MENU_OPEN, 'the track menu');
  const items = ev(page, `[...document.querySelectorAll('[data-testid=context-menu] [role=menuitem]')].map((b) => b.dataset.menuId)`);
  assert(items.includes('duplicate'), `menu items: ${items}`);
  page.run(['click', '[data-testid=context-menu] [data-menu-id=duplicate]']);
  const dup = await page.waitDebug((d) => d.project.tracks === start.project.tracks + 1, 'the duplicated track');
  assert(dup.project.undoDepth === renamed.project.undoDepth + 1, 'duplicate should be one undo step');
  assert(dup.project.regions > clipsBefore, 'the clips should be copied');
  assert(ev(page, NAMES)[1] === 'Lead line copy', `copy name: ${ev(page, NAMES)}`);
  out.duplicate = { tracks: dup.project.tracks, regions: dup.project.regions };
  press(page, 'Control+z');
  const undone = await page.waitDebug((d) => d.project.tracks === start.project.tracks, 'one undo to remove the copy');
  assert(undone.project.regions === clipsBefore, 'one undo should remove the copied clips too');

  // Reorder by keyboard: second grip, ArrowUp moves it to the top in one step.
  const before = ev(page, NAMES);
  ev(page, `(()=>{document.querySelectorAll('[data-track-id] [data-grip]')[1].focus();return true})()`);
  const depth = page.debug().project.undoDepth;
  press(page, 'ArrowUp');
  await page.waitDebug((d) => d.project.undoDepth === depth + 1, 'the reorder step');
  const after = ev(page, NAMES);
  assert(after[0] === before[1] && after[1] === before[0], `order after ArrowUp: ${after}`);
  assert(ev(page, `document.activeElement?.dataset.testid`) === 'track-grip', 'focus should stay on the grip');
  out.reorder = after;

  const errors = page.run(['errors'], { allowFail: true }).trim();
  assert(errors === '', `page errors: ${errors}`);
  return out;
}
