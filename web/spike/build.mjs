// Builds the wasm engine and copies it next to the page. No dependencies.
import { spawnSync } from 'node:child_process';
import { copyFileSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '..', '..');
const run = spawnSync(
  'cargo',
  ['build', '-p', 'composer-wasm', '--target', 'wasm32-unknown-unknown', '--release'],
  { cwd: repo, stdio: 'inherit' },
);
if (run.status !== 0) process.exit(run.status ?? 1);
const src = join(repo, 'target', 'wasm32-unknown-unknown', 'release', 'composer_wasm.wasm');
const dst = join(here, 'composer_wasm.wasm');
copyFileSync(src, dst);
console.log(`copied ${dst} (${statSync(dst).size} bytes)`);
