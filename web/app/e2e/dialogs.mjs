// U33 e2e: toasts with Undo and stacking, confirm dialog focus (trap, Escape, return), unsaved work guard.
// Run: npm run build && E2E_PORT=4192 node e2e/dialogs.mjs
import { ab, check, finish, js, open, sleep, startPreview, until } from './shell-lib.mjs';

const trackCount = () => js(`document.querySelectorAll('[title="Delete track"]').length`);
const toastCount = () => js(`document.querySelectorAll('[data-testid="toast"]').length`);
const leavePrompted = () => js(`(() => { const e = new Event('beforeunload', { cancelable: true }); window.dispatchEvent(e); return e.defaultPrevented; })()`);
const confirmOpen = () => js(`document.querySelector('[data-testid="confirm-dialog"]').open`);
const insideConfirm = () => js(`document.querySelector('[data-testid="confirm-dialog"]').contains(document.activeElement)`);
const menuItemVisible = (id) => js(`document.querySelector('[data-menu-id="${id}"]').checkVisibility()`);

try {
  await startPreview();
  ab('set', 'viewport', '1280', '800');
  // Earlier runs autosave into the browser profile; start from the seeded project.
  open('?start=off');
  js(`(localStorage.clear(), true)`);
  open('?start=off');
  await until('workspace', () => js(`!!document.querySelector('[data-testid="dock-tab-notes"]')`));

  await until('tracks', () => trackCount() > 1);
  check('clean project does not prompt on leave', leavePrompted() === false);

  // Delete a track: the toast has Undo and Undo restores it.
  const before = trackCount();
  ab('click', '[title="Delete track"]');
  await until('track gone', () => trackCount() === before - 1);
  await until('toast', () => toastCount() === 1);
  check('delete track shows a toast', true);
  check('unsaved change prompts on leave', leavePrompted() === true);
  ab('click', '[data-testid="toast"] button');
  await until('track back', () => trackCount() === before);
  check('toast Undo restores the track', true);
  check('undo back to the saved state is clean again', leavePrompted() === false);

  // Two deletions stack two toasts.
  ab('click', '[title="Delete track"]');
  await until('first toast', () => toastCount() === 1);
  ab('click', '[title="Delete track"]');
  await until('two toasts', () => toastCount() === 2);
  check('two toasts stack', true);

  // Confirm dialog: focus inside, Tab stays inside, Escape cancels, focus returns to the opener.
  ab('click', '[data-testid="project-menu-button"]');
  await until('menu open', () => menuItemVisible('new'));
  ab('click', '[data-menu-id="new"]');
  await until('confirm open', () => confirmOpen());
  check('confirm dialog takes focus', insideConfirm());
  for (let i = 0; i < 4; i++) ab('press', 'Tab');
  check('Tab stays inside the confirm dialog', insideConfirm());
  ab('press', 'Escape');
  await until('confirm closed', () => !confirmOpen());
  check('Escape cancels and keeps the project', trackCount() === before - 2);
  check('focus returns to the opener', js(`document.activeElement?.dataset.testid`) === 'project-menu-button');

  // Confirming starts a new project, and the page is clean again.
  ab('click', '[data-testid="project-menu-button"]');
  await until('menu open again', () => menuItemVisible('new'));
  ab('click', '[data-menu-id="new"]');
  await until('confirm open again', () => confirmOpen());
  ab('click', '[data-testid="confirm-dialog"] [data-dialog-primary]');
  await until('confirm closed again', () => !confirmOpen());
  await sleep(300);
  check('new project is clean on leave', leavePrompted() === false);
  js(`(localStorage.clear(), true)`);
} catch (e) {
  check(`unexpected: ${e instanceof Error ? e.message : e}`, false);
}
finish();
