// U41 exit check: add Echo to a track, change a param, bypass it, reorder two devices, each one undo
// step and undo restores; then the master chain. Run one at a time with its own E2E_PORT and E2E_SESSION.
import { assert } from './lib.mjs';

const ev = (page, js) => {
  const out = page.run(['eval', js]).trim();
  try {
    const v = JSON.parse(out);
    return v;
  } catch {
    return out;
  }
};
const kinds = (page) =>
  ev(page, "JSON.stringify([...document.querySelectorAll('[data-testid=effect-card]')].map((c) => c.dataset.kind))");
const kindsOf = (page) => {
  const raw = kinds(page);
  return JSON.parse(typeof raw === 'string' ? raw : JSON.stringify(raw));
};
const valuetext = (page, card, param) =>
  ev(page, `document.querySelectorAll('[data-testid=effect-card]')[${card}]?.querySelector('[data-testid=effect-param-${param}]')?.getAttribute('aria-valuetext') ?? ''`);
const bypassed = (page, card) =>
  ev(page, `document.querySelectorAll('[data-testid=effect-card]')[${card}]?.dataset.bypassed ?? ''`);

async function step(page, expectDepth, what) {
  await page.waitDebug((d) => d.undoDepth === expectDepth, what);
}

export default async function effects({ page, url }) {
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off`);
  await page.waitDebug((d) => d.project && d.project.tracks >= 2, 'starter project with two tracks');
  const d0 = page.debug().undoDepth;
  page.run(['click', '[data-testid=dock-tab-effects]']);
  await page.waitDebug(() => String(ev(page, "!!document.querySelector('[data-testid=effects-panel]')")) === 'true', 'effects panel');

  // Add Echo from the menu: the empty chain offers quick buttons, the menu is the same list.
  page.run(['click', '[data-testid=effects-quick-delay]']);
  await step(page, d0 + 1, 'add Echo');
  page.run(['click', '[data-testid=effects-add]']);
  await page.waitDebug(() => String(ev(page, "document.querySelectorAll('[role=menu][aria-label^=Add] [role=menuitem]').length")) === '6', 'menu with six devices');
  const hints = ev(page, "[...document.querySelectorAll('[role=menu][aria-label^=Add] [role=menuitem]')].every((b) => b.textContent.length > 12)");
  assert(hints === true || hints === 'true', 'every menu item has a name and a one line description');
  page.run(['click', '[data-menu-id=gate]']);
  await step(page, d0 + 2, 'add Gate');
  assert(kindsOf(page).join() === 'delay,gate', `chain should be delay,gate, got ${kindsOf(page)}`);

  // Param: one arrow press is one command; Home resets to the default.
  page.run(['focus', '[data-testid=effect-card] [data-testid=effect-param-time_ms]']);
  page.run(['press', 'ArrowRight']);
  await step(page, d0 + 3, 'param command');
  assert(valuetext(page, 0, 'time_ms') === '251 ms', `time should read 251 ms, got ${valuetext(page, 0, 'time_ms')}`);
  page.run(['press', 'Home']);
  await step(page, d0 + 4, 'param reset');
  assert(valuetext(page, 0, 'time_ms') === '250 ms', 'Home should reset the time to 250 ms');

  // Bypass.
  page.run(['click', '[data-testid=effect-card] [data-testid=effect-bypass]']);
  await step(page, d0 + 5, 'bypass');
  assert(bypassed(page, 0) === 'true', 'first device should be bypassed');

  // Reorder by the Move earlier button of the second card, then back with Alt+ArrowDown on the grip.
  page.run(['eval', "document.querySelectorAll('[data-testid=effect-card]')[1].querySelector('[data-testid=effect-up]').click()"]);
  await step(page, d0 + 6, 'move up');
  assert(kindsOf(page).join() === 'gate,delay', `after move up got ${kindsOf(page)}`);
  page.run(['focus', '[data-testid=effect-card] [data-testid=effect-grip]']);
  page.run(['press', 'Alt+ArrowDown']);
  await step(page, d0 + 7, 'move down by keyboard');
  assert(kindsOf(page).join() === 'delay,gate', `after Alt+Down got ${kindsOf(page)}`);

  // Undo walks back one step at a time.
  const undo = () => page.run(['eval', "document.querySelector('button[aria-label^=\"Undo\"]').click()"]);
  undo();
  await step(page, d0 + 6, 'undo move');
  assert(kindsOf(page).join() === 'gate,delay', 'undo restores the order before the keyboard move');
  undo();
  await step(page, d0 + 5, 'undo move up');
  assert(kindsOf(page).join() === 'delay,gate', 'undo restores the original order');
  undo();
  await step(page, d0 + 4, 'undo bypass');
  assert(bypassed(page, 0) === 'false', 'undo clears the bypass');
  undo();
  await step(page, d0 + 3, 'undo reset');
  assert(valuetext(page, 0, 'time_ms') === '251 ms', 'undo of the reset gives 251 ms back');
  undo();
  await step(page, d0 + 2, 'undo param');
  assert(valuetext(page, 0, 'time_ms') === '250 ms', 'undo restores 250 ms');
  undo();
  undo();
  await step(page, d0, 'undo both adds');
  assert(kindsOf(page).length === 0, 'chain is empty again');

  // Master chain.
  page.run(['select', '[data-testid=effects-owner]', 'master']);
  page.run(['click', '[data-testid=effects-quick-waveshaper]']);
  await step(page, d0 + 1, 'master add');
  assert(kindsOf(page).join() === 'waveshaper', 'master chain holds Distortion');
  page.run(['focus', '[data-testid=effect-card] [data-testid=effect-param-drive]']);
  page.run(['press', 'ArrowRight']);
  await step(page, d0 + 2, 'master param');
  assert(valuetext(page, 0, 'drive') === '31%', `drive should read 31%, got ${valuetext(page, 0, 'drive')}`);
  // Drag: HTML5 drag events from one grip onto the next card, one undo step.
  page.run(['click', '[data-testid=effects-add]']);
  page.run(['click', '[data-menu-id=tidal]']);
  await step(page, d0 + 3, 'master add second');
  const fire = (target, type) =>
    `(()=>{const dt=window.__dt??(window.__dt=new DataTransfer());${target}.dispatchEvent(new DragEvent('${type}',{bubbles:true,cancelable:true,dataTransfer:dt}));return true})()`;
  const card = (i) => `document.querySelectorAll('[data-testid=effect-card]')[${i}]`;
  const grip = `${card(0)}.querySelector('[data-testid=effect-grip]')`;
  page.run(['eval', fire(grip, 'dragstart')]);
  page.run(['eval', fire(card(1), 'dragover')]);
  page.run(['eval', fire(card(1), 'drop')]);
  page.run(['eval', fire(grip, 'dragend')]);
  await step(page, d0 + 4, 'drag reorder');
  assert(kindsOf(page).join() === 'tidal,waveshaper', `after drag got ${kindsOf(page)}`);
  // A track chain stays separate from the master chain.
  page.run(['select', '[data-testid=effects-owner]', ev(page, "document.querySelector('[data-testid=effects-owner] option').value")]);
  assert(kindsOf(page).length === 0, 'the first track has no devices of its own');
  return { steps: 'add, param, reset, bypass, move x2, undo x7, master add and param' };
}
