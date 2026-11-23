// Usage: node e2e/run.mjs <script> (e.g. smoke), from web/app after npm install and npm run wasm.
// Builds, serves vite preview, runs e2e/<script>.mjs (default export async ({page, url}) => result), exits 1 on failure.
// Env: E2E_SKIP_BUILD, E2E_WAIT_MS, E2E_CMD_TIMEOUT_MS, VITE_E2E_FIRST_SOUND_MS (budget, default 10000).
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { Page, startPreview } from './lib.mjs';

const name = process.argv[2];
const file = name ? join(dirname(fileURLToPath(import.meta.url)), `${name}.mjs`) : '';
if (!name || name === 'lib' || name === 'run' || !existsSync(file)) {
  process.stderr.write('usage: node e2e/run.mjs <script>   (e2e/<script>.mjs must exist)\n');
  process.exit(2);
}

let server = null;
const page = new Page();
let code = 0;
try {
  server = await startPreview();
  const script = (await import(pathToFileURL(file).href)).default;
  const result = await script({ page, url: server.url });
  process.stdout.write(`${JSON.stringify({ script: name, ok: true, ...result })}\n`);
} catch (e) {
  process.stdout.write(`${JSON.stringify({ script: name, ok: false, error: e instanceof Error ? e.message : String(e) })}\n`);
  code = 1;
} finally {
  page.close();
  server?.stop();
}
process.exit(code);
