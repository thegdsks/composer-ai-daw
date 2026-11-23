// U27: scale pass. Opens a stress project, pans the arrangement for a few seconds, drags in the
// piano roll, makes small edits, and checks every measure against env budgets.
// Run: node e2e/run.mjs stress. Env: E2E_STRESS_NOTES (20000), E2E_STRESS_TRACKS (1), E2E_STRESS_REGIONS (1),
// E2E_PAN_MS (5000), VITE_OPEN_BUDGET_MS, VITE_FRAME_BUDGET_MS, VITE_INPUT_BUDGET_MS, VITE_HEAP_BUDGET_MB (unset = report only),
// E2E_BUDGETS=off reports numbers without failing (baseline runs).
import { assert, envNum } from './lib.mjs';
import { ev, sleep } from './clip-lib.mjs';

/** Runs an async page expression (a promise of a JSON-able value) and parses the answer. */
function evAsync(page, promiseExpr) {
  const out = page.run(['eval', `(async () => JSON.stringify(await ${promiseExpr}))()`]).trim();
  let v = JSON.parse(out);
  if (typeof v === 'string') v = JSON.parse(v);
  return v;
}

const BUDGETS = {
  open: envNum('VITE_OPEN_BUDGET_MS', 1500),
  frame: envNum('VITE_FRAME_BUDGET_MS', 8),
  input: envNum('VITE_INPUT_BUDGET_MS', 50),
  heapMb: envNum('VITE_HEAP_BUDGET_MB', 0),
};

const round = (n) => (n === null || n === undefined ? null : Math.round(n * 100) / 100);

export default async function stress({ page, url }) {
  const notes = envNum('E2E_STRESS_NOTES', 20000);
  const tracks = envNum('E2E_STRESS_TRACKS', 1);
  const regions = envNum('E2E_STRESS_REGIONS', 1);
  const panMs = envNum('E2E_PAN_MS', 5000);
  page.run(['set', 'viewport', '1280', '800']);
  page.open(`${url}?e2e=1&start=off&stress=${notes}&tracks=${tracks}&regions=${regions}`);
  const ready = await page.waitDebug((d) => d.project && d.project.notes >= notes && d.arrangement, 'stress project', envNum('E2E_WAIT_MS', 120000));
  const out = {
    command: `E2E_STRESS_NOTES=${notes} E2E_STRESS_TRACKS=${tracks} E2E_STRESS_REGIONS=${regions} node e2e/run.mjs stress`,
    project: { tracks: ready.project.tracks, regions: ready.project.regions, notes: ready.project.notes },
    openMs: round(ready.readyMs),
    heapAfterOpenMb: ready.heapBytes === null ? null : round(ready.heapBytes / 1048576),
  };

  // Pan: wheel events at animation frame rate for panMs, arrangement draw times recorded.
  page.run(['eval', 'window.composerDebug.resetStats()']);
  const pan = evAsync(
    page,
    `new Promise((resolve) => {
      const c = document.querySelector('#arrangement-canvas');
      const t0 = performance.now();
      let n = 0;
      const step = () => {
        const dir = Math.floor((performance.now() - t0) / 1000) % 2 === 0 ? 1 : -1;
        c.dispatchEvent(new WheelEvent('wheel', { deltaX: 90 * dir, deltaY: 0, bubbles: true, cancelable: true }));
        n++;
        if (performance.now() - t0 < ${panMs}) requestAnimationFrame(step); else resolve({ events: n });
      };
      requestAnimationFrame(step);
    }).then((r) => ({ ...r, draw: window.composerDebug.arrangementDraw }))`,
  );
  out.pan = { events: pan.events, frames: pan.draw.frames, p95Ms: round(pan.draw.p95Ms), avgMs: round(pan.draw.avgMs), maxMs: round(pan.draw.maxMs) };

  // Roll drag: real press, synthetic moves for panMs, real release (one commit).
  const box = ev(page, `(() => { const r = document.querySelector('#piano-roll').getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; })()`);
  const start = { x: Math.round(box.x + 52 + 200), y: Math.round(box.y + 28 + 100) };
  page.run(['eval', 'window.composerDebug.resetStats()']);
  const edits0 = page.debug().edits.count;
  page.run(['mouse', 'move', String(start.x), String(start.y)]);
  page.run(['mouse', 'down']);
  const drag = evAsync(
    page,
    `new Promise((resolve) => {
      const c = document.querySelector('#piano-roll');
      const t0 = performance.now();
      const step = () => {
        const f = Math.min(1, (performance.now() - t0) / ${panMs});
        c.dispatchEvent(new PointerEvent('pointermove', { clientX: ${start.x} + Math.round(f * 300), clientY: ${start.y} + Math.round(f * 30), pointerId: 1, buttons: 1, bubbles: true }));
        if (f < 1) requestAnimationFrame(step); else resolve(true);
      };
      requestAnimationFrame(step);
    }).then(() => window.composerDebug.roll)`,
  );
  page.run(['mouse', 'up']);
  out.rollDrag = { frames: drag.frames, p95Ms: round(drag.p95Ms), avgMs: round(drag.avgMs), maxMs: round(drag.maxMs) };
  const after = await page.waitDebug((d) => d.edits.count > edits0, 'drag commit', 20000);
  out.dragCommitMs = round(after.edits.lastMs);

  // Small edits: undo and redo of the drag, each measured send to refreshed store.
  page.run(['press', 'Control+z']);
  const undone = await page.waitDebug((d) => d.edits.count > edits0 + 1, 'undo', 20000);
  out.undoMs = round(undone.edits.lastMs);
  page.run(['press', 'Control+Shift+z']);
  const redone = await page.waitDebug((d) => d.edits.count > edits0 + 2, 'redo', 20000);
  out.redoMs = round(redone.edits.lastMs);
  await sleep(300);
  const end = page.debug();
  out.heapEndMb = end.heapBytes === null ? null : round(end.heapBytes / 1048576);
  out.longTasks = end.longTasks;
  out.budgets = BUDGETS;

  if (process.env.E2E_BUDGETS !== 'off') {
    assert(out.openMs <= BUDGETS.open, `open ${out.openMs} ms over VITE_OPEN_BUDGET_MS ${BUDGETS.open}`);
    assert(out.pan.p95Ms <= BUDGETS.frame, `pan p95 draw ${out.pan.p95Ms} ms over VITE_FRAME_BUDGET_MS ${BUDGETS.frame}`);
    assert(out.rollDrag.p95Ms <= BUDGETS.frame, `roll drag p95 draw ${out.rollDrag.p95Ms} ms over VITE_FRAME_BUDGET_MS ${BUDGETS.frame}`);
    for (const k of ['dragCommitMs', 'undoMs', 'redoMs']) assert(out[k] <= BUDGETS.input, `${k} ${out[k]} ms over VITE_INPUT_BUDGET_MS ${BUDGETS.input}`);
    if (BUDGETS.heapMb > 0 && out.heapEndMb !== null) assert(out.heapEndMb <= BUDGETS.heapMb, `heap ${out.heapEndMb} MB over VITE_HEAP_BUDGET_MB ${BUDGETS.heapMb}`);
  }
  return out;
}
