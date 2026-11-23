// U32: a sound reaches a track by keyboard (row menu, the twin of a drop) and by HTML5 drag events, each one undo step.
// Run: E2E_PORT=<free port> E2E_SESSION=<name> node e2e/run.mjs dnd
import { assert } from './lib.mjs';
import { boot, ev, press, sleep } from './clip-lib.mjs';

const ACTIVE_SOUND = `document.querySelector('[data-testid=library-sound][data-active=true]')?.dataset.sound ?? null`;
const MENU_OPEN = `Boolean(document.querySelector('[data-testid=context-menu]:popover-open'))`;
const STRIP = `document.querySelector('[data-testid=drop-new-track]')`;

async function until(page, expr, what) {
  const deadline = Date.now() + 8000;
  while (!ev(page, expr)) {
    assert(Date.now() < deadline, `timed out waiting for ${what}`);
    await sleep(100);
  }
}

export default async function dnd({ page, url }) {
  await boot(page, url);
  const start = page.debug().project;
  assert(start.tracks >= 2, 'need two tracks');
  const out = {};

  // Keyboard only: search box, arrows into the list, Shift+F10, pick the second track, Enter.
  page.run(['click', 'input[type=search]']);
  press(page, 'ArrowDown');
  press(page, 'ArrowDown');
  const sound = ev(page, ACTIVE_SOUND);
  assert(sound, 'arrow keys should land on a sound');
  press(page, 'Shift+F10');
  await until(page, MENU_OPEN, 'the sound menu');
  press(page, 'ArrowDown');
  press(page, 'Enter');
  const used = await page.waitDebug((d) => d.project.sounds[1] === sound, 'sound on track 2');
  assert(used.project.undoDepth === start.undoDepth + 1, `keyboard drop should be one undo step, got ${used.project.undoDepth - start.undoDepth}`);
  assert(ev(page, MENU_OPEN) === false, 'menu should close after Enter');
  assert(ev(page, `document.activeElement?.getAttribute('data-testid')`) === 'library-list', 'focus should return to the sound list');
  out.keyboardDrop = { sound, depth: used.project.undoDepth };

  page.click('Undo');
  const undone = await page.waitDebug((d) => d.project.undoDepth === start.undoDepth, 'undo of the drop');
  assert(undone.project.sounds[1] !== sound, 'one undo should restore track 2');

  // Pointer path: HTML5 drag events from a sound row onto a track header, then onto the new track strip.
  const second = ev(page, `document.querySelectorAll('[data-testid=library-sound]')[1].dataset.sound`);
  const fire = (target, type) =>
    `(()=>{const dt=window.__dt??(window.__dt=new DataTransfer());${target}.dispatchEvent(new DragEvent('${type}',{bubbles:true,cancelable:true,dataTransfer:dt}));return true})()`;
  const row = `document.querySelector('[data-sound="${second}"]')`;
  const header = `document.querySelectorAll('[data-track-id]')[0]`;
  ev(page, fire(row, 'dragstart'));
  await until(page, `Boolean(${STRIP})`, 'the new track strip');
  ev(page, fire(header, 'dragover'));
  ev(page, fire(header, 'drop'));
  const onTrack = await page.waitDebug((d) => d.project.sounds[0] === second, 'sound on track 1 by drag');
  assert(onTrack.project.undoDepth === start.undoDepth + 1, 'drag onto a track should be one undo step');
  ev(page, fire(row, 'dragend'));

  ev(page, fire(row, 'dragstart'));
  await until(page, `Boolean(${STRIP})`, 'the new track strip');
  ev(page, fire(STRIP, 'dragover'));
  ev(page, fire(STRIP, 'drop'));
  const added = await page.waitDebug((d) => d.project.tracks === start.tracks + 1, 'a new track');
  assert(added.project.sounds[start.tracks] === second, 'the new track should carry the dropped sound');
  assert(added.project.undoDepth === start.undoDepth + 2, 'new track drop should be one more undo step');
  ev(page, fire(row, 'dragend'));
  await until(page, `!${STRIP}`, 'the strip to go away');
  out.pointerDrop = { onTrack: onTrack.project.undoDepth, newTrackDepth: added.project.undoDepth };

  const errors = page.run(['errors'], { allowFail: true }).trim();
  assert(errors === '', `page errors: ${errors}`);
  return out;
}
