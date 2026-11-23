// Small e2e kit: build and serve the app, drive agent-browser, read composerDebug, assert.
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const app = join(dirname(fileURLToPath(import.meta.url)), '..');
const vite = join(app, 'node_modules/vite/bin/vite.js');
const bin = process.env.AGENT_BROWSER_BIN ?? 'agent-browser';

export function envNum(key, fallback) {
  const raw = process.env[key];
  const n = Number(raw);
  return raw !== undefined && raw !== '' && Number.isFinite(n) ? n : fallback;
}

export class Failure extends Error {}

export function assert(cond, message) {
  if (!cond) throw new Failure(message);
}

function freePort() {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.once('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

/** Builds the app (unless E2E_SKIP_BUILD is set) and starts vite preview on a free port. */
export async function startPreview() {
  if (!process.env.E2E_SKIP_BUILD) {
    const b = spawnSync(process.execPath, [vite, 'build'], { cwd: app, encoding: 'utf8' });
    assert(b.status === 0, `vite build failed:\n${b.stdout}${b.stderr}`);
  }
  const port = envNum('E2E_PORT', 0) || (await freePort());
  const child = spawn(process.execPath, [vite, 'preview', '--port', String(port), '--strictPort'], { cwd: app, stdio: 'ignore' });
  const url = `http://127.0.0.1:${port}/`;
  const deadline = Date.now() + envNum('E2E_SERVER_TIMEOUT_MS', 15000);
  for (;;) {
    try {
      if ((await fetch(url)).ok) break;
    } catch {
      // not listening yet
    }
    if (Date.now() >= deadline) {
      child.kill();
      throw new Failure('preview server did not start');
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  return { url, stop: () => child.kill() };
}

// agent-browser may print an eval string JSON-quoted, so unwrap one level when needed.
function tryParse(text) {
  if (!text) return null;
  try {
    let v = JSON.parse(text);
    if (typeof v === 'string') v = JSON.parse(v);
    return v && typeof v === 'object' ? v : null;
  } catch {
    return null;
  }
}

/** One browser session. Every call shells out to agent-browser with this session name. */
export class Page {
  // pid plus a random tag keeps two Pages in one process, or a reused pid, from sharing a browser.
  constructor(session = process.env.E2E_SESSION || `composer-e2e-${process.pid}-${Math.random().toString(36).slice(2, 7)}`) {
    this.session = session;
  }

  run(args, { allowFail = false } = {}) {
    const r = spawnSync(bin, ['--session', this.session, ...args], { encoding: 'utf8', timeout: envNum('E2E_CMD_TIMEOUT_MS', 30000) });
    if (r.error) throw new Failure(`agent-browser ${args[0]}: ${r.error.message}`);
    if (r.status !== 0 && !allowFail) throw new Failure(`agent-browser ${args.join(' ')} failed: ${r.stdout}${r.stderr}`.trim());
    return r.stdout;
  }

  open(url) {
    this.run(['open', url]);
  }

  /** Compact interactive snapshot text. */
  snapshot() {
    return this.run(['snapshot', '-i', '-c']);
  }

  /** Ref (@eN) of the element named exactly like the label, else the first enabled one whose name starts with it, or null. */
  findRef(label) {
    const lines = this.snapshot().split('\n');
    const refOf = (line) => line.match(/ref=(e\d+)/)?.[1];
    for (const line of lines) {
      if (!line.includes(`"${label}"`)) continue;
      const r = refOf(line);
      if (r) return `@${r}`;
    }
    for (const line of lines) {
      if (!line.includes(`"${label}`) || line.includes('disabled')) continue;
      const r = refOf(line);
      if (r) return `@${r}`;
    }
    return null;
  }

  click(label) {
    const ref = this.findRef(label);
    assert(ref, `no element labelled "${label}" in the snapshot`);
    this.run(['click', ref]);
  }

  /** composerDebug as an object: eval first, the ?e2e data attribute dump when eval is blocked. */
  debug() {
    // A read is idempotent, so a page that is still navigating gets a few quick retries.
    for (let i = 0; i < 3; i++) {
      const viaEval = tryParse(this.run(['eval', 'JSON.stringify(window.composerDebug)'], { allowFail: true }).trim());
      if (viaEval) return viaEval;
    }
    const viaAttr = tryParse(this.run(['get', 'attr', 'html', 'data-composer-debug'], { allowFail: true }).trim());
    assert(viaAttr, 'composerDebug unreadable (eval and data-composer-debug both failed)');
    return viaAttr;
  }

  /** Polls composerDebug until pick(debug) is truthy and returns that object. */
  async waitDebug(pick, what, timeoutMs = envNum('E2E_WAIT_MS', 15000)) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      let d = null;
      try {
        d = this.debug();
      } catch {
        // page still booting
      }
      if (d && pick(d)) return d;
      assert(Date.now() < deadline, `timed out waiting for ${what}`);
      await new Promise((r) => setTimeout(r, 100));
    }
  }

  close() {
    this.run(['close'], { allowFail: true });
  }
}
