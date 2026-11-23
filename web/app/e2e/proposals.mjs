// U26: AI assist in the palette. A proposal posted from outside (the driver stands in for an AI
// client) shows as a badge and a card; Preview leaves the state hash unchanged after it stops;
// Accept adds exactly one history step tagged ai; the card Undo restores the hash; a stale
// proposal cannot be accepted; a local recipe card applies as one step tagged recipe.
import { assert } from './lib.mjs';
import { ev, press, sleep, waitDepth } from './clip-lib.mjs';

const track = (n, name) => ({ command: 'add_track', id: `a${String(n).padStart(31, '0')}`, name, kind: 'instrument', index: null });

async function until(page, expr, what, ms = 8000) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = ev(page, `(() => { const v = (${expr}); return v instanceof Element ? true : v; })()`);
    if (v) return v;
    assert(Date.now() < deadline, `timed out waiting for ${what}`);
    await sleep(100);
  }
}

const click = (page, selector) => ev(page, `(() => { const e = document.querySelector('${selector}'); if (!e) return false; e.click(); return true; })()`);
const text = (page, selector) => ev(page, `document.querySelector('${selector}') ? document.querySelector('${selector}').textContent : null`);

async function post(page, label, commands, note) {
  ev(page, `(window.composerE2e.post(${JSON.stringify(label)}, ${JSON.stringify(commands)}, ${JSON.stringify(note)}, 'agent'), true)`);
  await until(page, `window.composerE2e.lastPost !== null && window.composerE2e.lastPost.startsWith('ok:')`, `post of ${label}`);
}

async function sources(page) {
  ev(page, '(window.composerE2e.loadSources(), true)');
  await sleep(300);
  return ev(page, 'window.composerE2e.sources');
}

async function openPalette(page) {
  press(page, 'Control+k');
  await until(page, `document.querySelector('[data-testid=palette-dialog]').open`, 'palette open');
  await until(page, `document.querySelectorAll('[data-testid=palette-item]').length > 20`, 'palette list');
}

export default async function proposals({ page, url }) {
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off`);
  const start = await page.waitDebug((d) => d.project && d.project.tracks > 0, 'starter project');
  const hash0 = start.project.hash;
  const depth0 = start.undoDepth;
  const tracks0 = start.project.tracks;

  // 1. Nothing waiting: no badge. Post one proposal: the project must not change.
  assert(ev(page, `document.querySelector('[data-testid=assist-badge]') === null`), 'no badge before a proposal');
  await post(page, 'Add a pulse track', [track(1, 'Pulse')], 'A steady pulse under the pad.');
  assert(page.debug().project.hash === hash0, 'posting must not change the project');
  await until(page, `document.querySelector('[data-testid=assist-badge]') && document.querySelector('[data-testid=assist-badge]').textContent === '1'`, 'badge showing 1');
  const chip = await until(page, `document.querySelector('[data-testid=next-action]') && document.querySelector('[data-testid=next-action]').textContent`, 'next action chip');
  assert(/suggestion is waiting/i.test(chip), `the chip should read the waiting list: ${chip}`);
  page.run(['screenshot', '/tmp/composer-assist-badge.png'], { allowFail: true });

  // 2. The palette lists it first, with the AI tag, plus local recipe cards.
  await openPalette(page);
  const items = ev(page, `[...document.querySelectorAll('[data-testid=palette-item]')].map(l => ({ id: l.dataset.command, text: l.textContent }))`);
  assert(items[0].id === 'assist:proposal:p1', `proposal should come first, got ${items[0].id}`);
  assert(/AI/.test(items[0].text), `row should carry the AI tag: ${items[0].text}`);
  const recipeCards = items.filter((i) => i.id.startsWith('assist:recipe:')).map((i) => i.id);
  assert(recipeCards.length > 0, `expected local recipe suggestions, got ${JSON.stringify(items.slice(0, 8).map((i) => i.id))}`);
  page.run(['screenshot', '/tmp/composer-assist-list.png'], { allowFail: true });

  // 3. Open the card: plain summary, counts, disclosed commands.
  click(page, '[data-command="assist:proposal:p1"]');
  await until(page, `document.querySelector('[data-testid=assist-card]')`, 'card');
  const changes = text(page, '[data-testid=assist-changes]');
  assert(/Adds 1 track/.test(changes), `summary should say what changes: ${changes}`);
  assert(/1 command, \d+ edits? to the project/.test(text(page, '[data-testid=assist-counts]')), 'counts line missing');
  assert(ev(page, `document.querySelector('[data-testid=assist-commands]').textContent.includes('add_track')`), 'disclosed commands should list add_track');
  assert(ev(page, `document.querySelectorAll('[data-primary=true]').length`) >= 1, 'one primary action');

  // 4. Preview shows the change live, Stop preview restores the exact hash, depth never moved.
  click(page, '[data-testid=assist-preview]');
  await page.waitDebug((d) => d.project.tracks === tracks0 + 1, 'preview shows the new track');
  assert(page.debug().undoDepth === depth0, 'a preview must not add a history step');
  click(page, '[data-testid=assist-preview]');
  await page.waitDebug((d) => d.project.hash === hash0, 'hash restored after stopping the preview');
  assert(page.debug().project.tracks === tracks0, 'track count restored');

  // 5. Preview again, then Accept: exactly one step, tagged ai; the card Undo restores the hash.
  click(page, '[data-testid=assist-preview]');
  await page.waitDebug((d) => d.project.tracks === tracks0 + 1, 'second preview');
  click(page, '[data-testid=assist-accept]');
  await waitDepth(page, depth0 + 1);
  assert(page.debug().project.tracks === tracks0 + 1, 'accepted proposal adds the track once');
  const afterAccept = await sources(page);
  assert(afterAccept.at(-1) === 'ai:Add a pulse track', `last step should be tagged ai: ${afterAccept.at(-1)}`);
  assert(afterAccept.length === depth0 + 1 || afterAccept.length >= depth0 + 1, 'history grew');
  await until(page, `document.querySelector('[data-testid=assist-undo]')`, 'card Undo button');
  page.run(['screenshot', '/tmp/composer-assist-applied.png'], { allowFail: true });
  click(page, '[data-testid=assist-undo]');
  await page.waitDebug((d) => d.project.hash === hash0, 'card Undo restores the hash');
  assert(page.debug().undoDepth === depth0, 'depth back to the start');
  click(page, '[data-testid=assist-card] [data-primary=true]');
  await until(page, `!document.querySelector('[data-testid=palette-dialog]').open`, 'palette closed');
  await until(page, `document.querySelector('[data-testid=assist-badge]') === null`, 'badge gone after resolving');

  // 6. Stale: two proposals add the same track id. After one is accepted the other no longer fits.
  await post(page, 'Add a drone track', [track(2, 'Drone')], 'A low drone.');
  await post(page, 'Add a drone track again', [track(2, 'Drone')], 'Same id, so it will clash.');
  await openPalette(page);
  click(page, '[data-command="assist:proposal:p2"]');
  await until(page, `document.querySelector('[data-testid=assist-card]')`, 'card p2');
  click(page, '[data-testid=assist-accept]');
  await waitDepth(page, depth0 + 1);
  click(page, '[data-testid=assist-card] [data-primary=true]');
  await until(page, `!document.querySelector('[data-testid=palette-dialog]').open`, 'palette closed again');
  const hash1 = page.debug().project.hash;
  await openPalette(page);
  click(page, '[data-command="assist:proposal:p3"]');
  await until(page, `document.querySelector('[data-testid=assist-card]')`, 'card p3');
  const blocked = await until(page, `document.querySelector('[data-testid=assist-blocked]') && document.querySelector('[data-testid=assist-blocked]').textContent`, 'blocked note');
  assert(/no longer fits/i.test(blocked), `stale card should explain itself: ${blocked}`);
  assert(ev(page, `document.querySelector('[data-testid=assist-accept]').disabled`), 'accept must be disabled when it no longer fits');
  assert(ev(page, `document.querySelector('[data-testid=assist-preview]').disabled`), 'preview must be disabled when it no longer fits');
  click(page, '[data-testid=assist-reject]');
  await until(page, `/Rejected/.test(document.querySelector('[data-testid=assist-status]').textContent)`, 'rejected message');
  assert(page.debug().project.hash === hash1, 'reject must not touch the project');
  page.run(['screenshot', '/tmp/composer-assist-stale.png'], { allowFail: true });
  click(page, '[data-testid=assist-card] [data-primary=true]');
  await until(page, `document.querySelector('[data-testid=assist-card]') === null`, 'back to the list');

  // 7. A local recipe card: preview leaves the hash alone, apply is one step tagged recipe.
  const depth2 = page.debug().undoDepth;
  const hash2 = page.debug().project.hash;
  const recipe = ev(page, `[...document.querySelectorAll('[data-testid=palette-item]')].map(l => l.dataset.command).find(c => c.startsWith('assist:recipe:'))`);
  assert(recipe, 'a recipe card should still be offered');
  click(page, `[data-command="${recipe}"]`);
  await until(page, `document.querySelector('[data-testid=assist-card][data-origin=recipe]')`, 'recipe card');
  assert(/Recipe/.test(text(page, '[data-testid=assist-tag]')), 'recipe tag');
  click(page, '[data-testid=assist-preview]');
  await until(page, `document.querySelector('[data-testid=assist-preview]').getAttribute('aria-pressed') === 'true'`, 'recipe preview on');
  assert(page.debug().undoDepth === depth2, 'recipe preview adds no step');
  click(page, '[data-testid=assist-preview]');
  await page.waitDebug((d) => d.project.hash === hash2, 'hash restored after recipe preview');
  click(page, '[data-testid=assist-accept]');
  await waitDepth(page, depth2 + 1);
  const afterRecipe = await sources(page);
  assert(afterRecipe.at(-1).startsWith('recipe:'), `last step should be tagged recipe: ${afterRecipe.at(-1)}`);

  // 8. Escape while previewing rolls the preview back.
  click(page, '[data-testid=assist-card] [data-primary=true]');
  await until(page, `!document.querySelector('[data-testid=palette-dialog]').open`, 'palette closed');
  const hash3 = page.debug().project.hash;
  await post(page, 'Add a last track', [track(9, 'Last')], 'For the escape check.');
  await openPalette(page);
  const next = ev(page, `document.querySelector('[data-command^="assist:proposal:"]').dataset.command`);
  click(page, `[data-command="${next}"]`);
  await until(page, `document.querySelector('[data-testid=assist-card]')`, 'last card');
  click(page, '[data-testid=assist-preview]');
  await page.waitDebug((d) => d.project.hash !== hash3, 'preview before escape');
  press(page, 'Escape');
  await page.waitDebug((d) => d.project.hash === hash3, 'escape rolled the preview back');

  return { proposalSteps: afterAccept.at(-1), recipeStep: afterRecipe.at(-1), recipeCards: recipeCards.length };
}
