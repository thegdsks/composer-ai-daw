// U01 e2e: dock tabs, theme toggle, remembered split, live announcer.
// Run: npm run build && node e2e/shell.mjs   (starts vite preview on E2E_PORT, default a free port)
import { ab, check, finish, js, open, ready, startPreview, until } from './shell-lib.mjs';

const attr = (testid, name) => js(`document.querySelector('[data-testid="${testid}"]').getAttribute('${name}')`);
const split = () => Number(attr('splitter', 'aria-valuenow'));

try {
  await startPreview();
  ab('set', 'viewport', '1280', '800');
  // Earlier runs persist the split and theme in the browser profile; start clean.
  open('?start=off');
  js(`(localStorage.clear(), true)`);
  open('?start=off');
  await ready();

  // Dock tabs switch and show one panel at a time.
  for (const [tab, text] of [
    ['sound', 'Sound for'],
    ['mix', 'Mixer strips'],
    ['notes', 'Piano roll'],
  ]) {
    ab('click', `[data-testid="dock-tab-${tab}"]`);
    const selected = await until(`${tab} selected`, () => attr(`dock-tab-${tab}`, 'aria-selected') === 'true');
    const visible = js(`[...document.querySelectorAll('[role="tabpanel"]')].filter((p) => !p.hidden).map((p) => p.id)`);
    const body = js(`document.querySelector('[role="tabpanel"]:not([hidden])').innerText + document.querySelector('[role="tabpanel"]:not([hidden])').getAttribute('aria-label')`);
    check(`dock tab ${tab} switches`, selected && visible.length === 1 && visible[0] === `dock-panel-${tab}`, String(body).replace(/\s+/g, ' ').slice(0, 30));
    const content =
      tab === 'notes'
        ? js(`!!document.querySelector('#piano-roll')`)
        : tab === 'mix'
          ? js(`!!document.querySelector('[role="tabpanel"]:not([hidden]) [aria-label="Mixer strips"]')`)
          : String(body).includes(text);
    check(`dock tab ${tab} shows its content`, content);
  }

  // Theme: settings menu sets data-theme and the color scheme follows.
  ab('click', '[data-testid="settings-button"]');
  await until('settings open', () => js(`document.querySelector('#settings-popover').matches(':popover-open')`));
  ab('click', 'input[name="theme"][value="dark"]');
  await until('dark theme', () => js(`document.documentElement.dataset.theme`) === 'dark');
  check('theme dark sets data-theme', true);
  check('theme dark flips color-scheme', js(`getComputedStyle(document.documentElement).colorScheme`) === 'dark');
  ab('click', 'input[name="theme"][value="light"]');
  await until('light theme', () => js(`document.documentElement.dataset.theme`) === 'light');
  check('theme light sets data-theme', js(`getComputedStyle(document.documentElement).colorScheme`) === 'light');
  ab('click', 'input[name="theme"][value="system"]');
  await until('system theme', () => js(`document.documentElement.dataset.theme === undefined`));
  check('theme system removes data-theme', true);
  ab('press', 'Escape');

  // Split: drag the divider, reload, the position is remembered.
  const before = split();
  const box = js(`(() => { const r = document.querySelector('[data-testid="splitter"]').getBoundingClientRect(); return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) }; })()`);
  ab('mouse', 'move', String(box.x), String(box.y));
  ab('mouse', 'down');
  ab('mouse', 'move', String(box.x), String(box.y + 120));
  ab('mouse', 'up');
  await until('split moved', () => split() > before);
  const dragged = split();
  check('split drag moves the divider', dragged > before, `${before} to ${dragged}`);
  ab('reload');
  await ready();
  check('split persists across reload', split() === dragged, `${split()} vs ${dragged}`);

  // The arrangement keeps at least its minimum lanes when the divider is pushed to the top.
  ab('focus', '[data-testid="splitter"]');
  ab('press', 'Home');
  await until('split at top', () => split() < dragged);
  const minHeight = js(`document.querySelector('section[aria-label="Arrangement"]').getBoundingClientRect().height`);
  check('arrangement keeps its minimum lanes', minHeight >= 28 + 64 * 2, `${Math.round(minHeight)} px`);
  ab('press', 'Enter');

  // Announcer: a committed command is read as its step label, undo names the same label.
  ab('click', '#piano-roll');
  ab('press', 'Enter');
  const said = await until('announcement', () => {
    const t = js(`document.querySelector('[data-testid="announcer"]').textContent`);
    return t ? t : null;
  });
  const live = js(`document.querySelector('[data-testid="announcer"]').getAttribute('aria-live')`);
  check('announcer is a polite live region', live === 'polite');
  check('announcer reads a step label after adding a note', typeof said === 'string' && said.length > 0, said);
  ab('press', 'Control+z');
  const undone = await until('undo announcement', () => {
    const t = js(`document.querySelector('[data-testid="announcer"]').textContent`);
    return t && t !== said ? t : null;
  });
  check('announcer text equals the last step label', undone === `Undid ${said}`, undone);
} catch (e) {
  check('script ran to the end', false, e instanceof Error ? e.message : String(e));
}
finish();
