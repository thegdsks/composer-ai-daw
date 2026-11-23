// U15 e2e: Space on a focused Save to file button activates the button and does not play;
// Space on the page plays; Shift and / opens help; deleting a track offers Undo.
// Run: npm run build && E2E_PORT=4191 node e2e/plain.mjs
import { ab, check, finish, js, open, sleep, startPreview, until } from './shell-lib.mjs';

const transport = () => js(`document.querySelector('[data-testid="audio-state"]').dataset.transport`);
const footer = () => js(`document.querySelector('footer [role="status"]').innerText`);

try {
  await startPreview();
  ab('set', 'viewport', '1280', '800');
  open('?start=off');
  await until('workspace', () => js(`!!document.querySelector('[data-testid="dock-tab-notes"]')`));

  await until('tracks loaded', () => js(`document.querySelectorAll('[title="Delete track"]').length`) > 1);
  // A focused Save to file entry in the Project menu keeps Space for itself.
  ab('click', '[data-testid="project-menu-button"]');
  await until('menu open', () => js(`document.querySelector('[data-menu-id="save"]').checkVisibility()`));
  js(`(() => { const b = document.querySelector('[data-menu-id="save"]'); b.focus(); return document.activeElement === b; })()`);
  ab('press', 'Space');
  await until('save notice', () => /Saved /.test(footer()));
  await sleep(300);
  check('Space on Save to file activates the button', /Saved .*\.composer\.json/.test(footer()), footer());
  check('Space on Save to file does not start playback', transport() === 'stopped', transport());

  // On the page itself Space still plays and pauses.
  js(`(document.activeElement && document.activeElement.blur(), true)`);
  ab('press', 'Space');
  await until('playing', () => transport() === 'playing');
  check('Space on the page starts playback', true);
  ab('press', 'Space');
  await until('paused', () => transport() !== 'playing');
  check('Space again pauses', true);

  // Help overlay from the keymap.
  ab('press', 'Shift+Slash');
  await until('help open', () => js(`document.querySelector('[data-testid="help-dialog"]').open`));
  check('Shift and / opens the shortcut list', js(`document.querySelector('[data-testid="help-dialog"]').innerText.includes('Play or pause')`));
  ab('press', 'Escape');
  await until('help closed', () => !js(`document.querySelector('[data-testid="help-dialog"]').open`));

  // Deleting a track offers Undo in a toast.
  const before = js(`document.querySelectorAll('[aria-label^="Delete "][title="Delete track"]').length`);
  ab('click', '[title="Delete track"]');
  await until('toast', () => js(`!!document.querySelector('[data-testid="toast"]')`));
  check('one toast for one delete', js(`document.querySelectorAll('[data-testid="toast"]').length`) === 1);
  ab('click', '[data-testid="toast"] button');
  await until('track back', () => js(`document.querySelectorAll('[title="Delete track"]').length`) === before, 10000);
  check('delete track toast Undo brings the track back', true);

  // The edits above were autosaved, and the footer says where.
  await until('autosave time', () => /Saved in this browser at \d/.test(js(`document.querySelector('[data-testid="save-state"]').innerText`)), 6000);
  check('footer shows the autosave time', true);
} catch (e) {
  check(`unexpected: ${e instanceof Error ? e.message : e}`, false);
}
finish();
