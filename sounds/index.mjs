// Writes sounds/packs/index.json: the wired, fetched sampled sounds of sounds/catalogue.yaml,
// so the browser app can list them. Run by sounds/fetch.sh. Flat scalar fields only, like the
// catalogue reader in core/composer-catalogue.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const sounds = dirname(fileURLToPath(import.meta.url));
const unquote = (s) => (s === 'null' ? null : s.replace(/^"(.*)"$/, '$1').replace(/\\"/g, '"').replace(/\\\\/g, '\\'));
const entries = [];
for (const line of readFileSync(join(sounds, 'catalogue.yaml'), 'utf8').split('\n')) {
  const start = line.match(/^ {2}- id: (.*)$/);
  if (start) entries.push({ id: unquote(start[1]) });
  const kv = line.match(/^ {4}([a-z_]+): (.*)$/);
  const cur = entries[entries.length - 1];
  if (kv && cur) cur[kv[1]] = unquote(kv[2]);
}
const wired = entries.filter(
  (e) => e.kind === 'soundfont' && e.status === 'wired' && e.file && existsSync(join(sounds, 'packs', e.file)),
);
const out = wired.map((e) => ({
  id: e.id,
  name: e.name,
  kind: e.kind,
  status: e.status,
  file: e.file,
  licence: e.licence,
  instruments: e.instruments,
  bundle: e.bundle,
  url: e.url,
  attribution_required: e.attribution_required,
  attribution_text: e.attribution_text,
  approx_size: e.approx_size,
}));
writeFileSync(join(sounds, 'packs', 'index.json'), JSON.stringify(out, null, 2) + '\n');
console.log(`index.json: ${out.length} sampled sounds`);
