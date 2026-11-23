// Builds composer-wasm and copies the module to public/. Pass --if-missing to skip a file that is newer than the Rust sources.
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const app = join(here, '..');
const repo = join(app, '..', '..');
const dst = join(app, 'public', 'composer_wasm.wasm');

// A stale module after a merge fails tests in confusing ways, so compare against the sources.
function newestSource(root) {
  let newest = 0;
  for (const entry of readdirSync(root, { recursive: true })) {
    if (!/\.rs$|Cargo\.toml$/.test(entry)) continue;
    newest = Math.max(newest, statSync(join(root, entry)).mtimeMs);
  }
  return newest;
}
const fresh = existsSync(dst) && statSync(dst).mtimeMs >= newestSource(join(repo, 'core'));
if (process.argv.includes('--if-missing') && fresh) process.exit(0);

const profile = process.env.WASM_PROFILE ?? 'wasm-small';
const run = spawnSync(
  'cargo',
  ['build', '-p', 'composer-wasm', '--target', 'wasm32-unknown-unknown', '--profile', profile],
  { cwd: repo, stdio: 'inherit' },
);
if (run.status !== 0) process.exit(run.status ?? 1);
const dir = profile === 'dev' ? 'debug' : profile;
const src = join(repo, 'target', 'wasm32-unknown-unknown', dir, 'composer_wasm.wasm');
mkdirSync(dirname(dst), { recursive: true });
copyFileSync(src, dst);
console.log(`copied ${dst} (${statSync(dst).size} bytes)`);
