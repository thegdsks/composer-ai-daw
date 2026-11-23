// U32: context menus. A clip menu opens by right click and by Shift+F10, Escape closes it and gives focus back,
// Duplicate is one step; Delete track from the track menu shows a toast whose Undo restores it.
// Run: E2E_PORT=<free port> E2E_SESSION=<name> node e2e/run.mjs menu
import { assert } from './lib.mjs';
import { boot, ev, press, sleep } from './clip-lib.mjs';

const MENU_OPEN = `Boolean(document.querySelector('[data-testid=context-menu]:popover-open'))`;
const ITEMS = `[...document.querySelectorAll('[data-testid=context-menu] [role=menuitem]')].map((b) => b.dataset.menuId)`;

async function until(page, expr, what) {
  const deadline = Date.now() + 8000;
  while (!ev(page, expr)) {
    assert(Date.now() < deadline, `timed out waiting for ${what}`);
    await sleep(100);
  }
}

export default async function menu({ page, url }) {
  await boot(page, url);
  const start = page.debug();
  const out = {};

  // Right click on the first clip (a contextmenu event at its centre, the browser driver has no right button).
  const clip = start.arrangement.clips[0];
  const at = `{clientX: ${Math.round(clip.x + clip.width / 2)}, clientY: ${Math.round(clip.y + clip.height / 2)}}`;
  ev(page, `(()=>{const c=document.getElementById('arrangement-canvas');c.focus();c.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,cancelable:true,...${at}}));return true})()`);
  await until(page, MENU_OPEN, 'the clip menu');
  const items = ev(page, ITEMS);
  assert(JSON.stringify(items) === JSON.stringify(['open', 'loop', 'split', 'duplicate', 'delete']), `clip menu items: ${items}`);
  assert(ev(page, `document.activeElement?.getAttribute('role')`) === 'menuitem', 'focus should move into the menu');
  press(page, 'Escape');
  await until(page, `!${MENU_OPEN}`, 'Escape to close the menu');
  assert(ev(page, `document.activeElement?.id`) === 'arrangement-canvas', 'focus should return to the arrangement');

  // Same menu from the keyboard, then Duplicate by letter and Enter.
  press(page, 'Shift+F10');
  await until(page, MENU_OPEN, 'Shift+F10 menu');
  press(page, 'd');
  press(page, 'Enter');
  const dup = await page.waitDebug((d) => d.project.regions === start.project.regions + 1, 'a duplicated clip');
  assert(dup.project.undoDepth === start.project.undoDepth + 1, 'duplicate should be one undo step');
  out.duplicate = dup.project.regions;

  // Track menu from the keyboard: focus the name field, Shift+F10, Delete track.
  page.run(['click', '[data-track-id] [data-testid=track-name]']);
  press(page, 'Shift+F10');
  await until(page, MENU_OPEN, 'the track menu');
  const tItems = ev(page, ITEMS);
  assert(JSON.stringify(tItems) === JSON.stringify(['rename', 'move-up', 'move-down', 'duplicate', 'delete']), `track menu items: ${tItems}`);
  press(page, 'End');
  press(page, 'Enter');
  const gone = await page.waitDebug((d) => d.project.tracks === start.project.tracks - 1, 'the track to be deleted');
  await until(page, `document.body.innerText.includes('Track deleted')`, 'the Track deleted toast');
  page.click('Undo');
  const back = await page.waitDebug((d) => d.project.tracks === start.project.tracks, 'undo from the toast');
  out.deleteUndo = { after: gone.project.tracks, restored: back.project.tracks };

  const errors = page.run(['errors'], { allowFail: true }).trim();
  assert(errors === '', `page errors: ${errors}`);
  return out;
}
