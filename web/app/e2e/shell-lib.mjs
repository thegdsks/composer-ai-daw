// Local stand-in for the shared e2e harness (U02). It starts vite preview when nothing is
// listening, drives the agent-browser CLI and exits nonzero on failure. Replace with
// ./lib.mjs once that file is merged.
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const app = join(dirname(fileURLToPath(import.meta.url)), '..');
// Without E2E_PORT each run takes a free port, so parallel runs and stale servers never collide.
function freePort() {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.once('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const { port: p } = srv.address();
      srv.close(() => resolve(p));
    });
  });
}

export const port = process.env.E2E_PORT ? Number(process.env.E2E_PORT) : await freePort();
export const base = `http://127.0.0.1:${port}/`;
const session = process.env.E2E_SESSION ?? `composer-e2e-${process.pid}`;

let server = null;

async function listening() {
  try {
    const r = await fetch(base);
    return r.ok;
  } catch {
    return false;
  }
}

export async function startPreview() {
  // A given E2E_PORT may already serve a build; otherwise we own the server.
  if (process.env.E2E_PORT && (await listening())) return;
  server = spawn('npx', ['vite', 'preview', '--port', String(port), '--strictPort'], { cwd: app, stdio: 'ignore' });
  for (let i = 0; i < 50; i++) {
    if (await listening()) return;
    await sleep(200);
  }
  throw new Error('vite preview did not start (run npm run build first)');
}

export function stopPreview() {
  server?.kill();
  server = null;
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export function ab(...args) {
  const r = spawnSync('agent-browser', ['--session', session, ...args], { encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`agent-browser ${args.join(' ')} failed: ${r.stderr || r.stdout}`);
  return r.stdout.trim();
}

/** Runs a JS expression in the page and returns its JSON value. */
export function js(expr) {
  const out = ab('eval', `JSON.stringify(${expr})`);
  const text = out.split('\n').pop() ?? '';
  // The CLI prints the returned string as JSON, so there are exactly two layers to remove.
  const v = JSON.parse(JSON.parse(text));
  return v;
}

/** js() for pure reads: a page that is mid navigation or mid render may fail once, so try again. */
export function read(expr, tries = 3) {
  for (let i = 1; ; i++) {
    try {
      return js(expr);
    } catch (e) {
      if (i >= tries) throw e;
      spawnSync('sleep', ['0.2']);
    }
  }
}

/** Waits until the workspace is usable: tracks and piano roll mounted, fonts loaded, engine not in error. */
export async function ready(timeoutMs = 20000) {
  await until(
    'workspace ready',
    () => {
      try {
        return js(`!!document.querySelector('[data-testid="dock-tab-notes"]') && !!document.querySelector('[data-testid="splitter"]') && document.querySelectorAll('[title="Delete track"]').length > 0 && !!document.querySelector('#piano-roll') && document.fonts.status === 'loaded' && document.querySelector('[data-testid="audio-state"]')?.dataset.state !== 'error' && !document.querySelector('[data-testid="start-sheet"]')`);
      } catch {
        return false;
      }
    },
    timeoutMs,
  );
}

export async function until(label, probe, timeoutMs = Number(process.env.E2E_WAIT_MS ?? 8000)) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) {
    try {
      last = probe();
    } catch {
      last = null;
    }
    if (last) return last;
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${label}`);
}

const results = [];

export function check(name, ok, detail = '') {
  results.push({ name, ok });
  process.stdout.write(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` (${detail})` : ''}\n`);
}

export function finish() {
  try {
    ab('close');
  } catch {
    // the session is already gone
  }
  stopPreview();
  const failed = results.filter((r) => !r.ok).length;
  process.stdout.write(`${results.length - failed} passed, ${failed} failed\n`);
  process.exit(failed === 0 ? 0 : 1);
}

export function open(path = '') {
  ab('open', base + path);
}
