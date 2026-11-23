// W01: load a generated web plugin (gain) through the Project menu file input, play the starter
// with Loop on, and check the plugin changes the measured level: out/in peak follows the parameter.
// Run: node e2e/run.mjs wclap
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { assert, envNum } from './lib.mjs';
import { boot, ev, sleep } from './clip-lib.mjs';
import { buildGainPlugin } from '../tests/fixtures/gainPlugin.mjs';

const MIN_IN_PEAK = envNum('E2E_PLUGIN_MIN_IN_PEAK', 0.01);
const TOLERANCE = envNum('E2E_PLUGIN_RATIO_TOLERANCE', 0.03);

/** Waits for stats windows that postdate `afterBlocks` and carry signal, returns the out over in ratio. */
async function ratio(page, afterBlocks) {
  const deadline = Date.now() + envNum('E2E_WAIT_MS', 15000) * 2;
  for (;;) {
    const p = page.debug().audio.plugin;
    if (p && p.blocks > afterBlocks && p.inPeak > MIN_IN_PEAK) return { ratio: p.outPeak / p.inPeak, plugin: p };
    assert(Date.now() < deadline, `no stats window with signal after block ${afterBlocks}: ${JSON.stringify(p)}`);
    await sleep(100);
  }
}

export default async function wclap({ page, url }) {
  await boot(page, url);
  page.click('Loop');
  page.click('Play');
  await page.waitDebug((d) => d.audio.maxPeak > 0, 'the starter is audible');

  const dir = mkdtempSync(join(tmpdir(), 'wclap-e2e-'));
  const file = join(dir, 'gain.wclap');
  writeFileSync(file, buildGainPlugin());
  page.run(['upload', '[data-testid=plugin-file]', file]);
  const d = await page.waitDebug((x) => x.audio.plugin !== null, 'plugin loaded').catch((e) => {
    throw new Error(`${e.message}; audio ${JSON.stringify(page.debug().audio)}; toasts ${ev(page, 'document.body.innerText.slice(-300)')}`);
  });
  const info = d.audio.plugin;
  assert(info.name === 'Fixture gain', `plugin name ${info.name}`);
  assert(info.params.length === 1 && info.params[0].name === 'Gain', 'one Gain parameter');
  assert(info.nonDeterministic === true, 'plugin audio is flagged non deterministic');

  const unity = await ratio(page, info.blocks + 64);
  assert(Math.abs(unity.ratio - 1) <= TOLERANCE, `unity gain ratio ${unity.ratio}`);

  ev(page, `window.composerE2e.setPluginParam(0, 0.5)`);
  const mark = page.debug().audio.plugin.blocks + 96;
  const half = await ratio(page, mark);
  assert(Math.abs(half.ratio - 0.5) <= TOLERANCE, `half gain ratio ${half.ratio}`);

  const after = page.debug().audio;
  assert(after.plugin.bypassed === null, `plugin bypassed: ${after.plugin.bypassed}`);
  assert(after.state !== 'error', `audio error ${after.error}`);
  return {
    unityRatio: Number(unity.ratio.toFixed(3)),
    halfRatio: Number(half.ratio.toFixed(3)),
    loadMs: after.plugin.loadMs,
    avgBlockUs: Math.round(after.plugin.avgBlockUs),
    maxBlockUs: Math.round(after.plugin.maxBlockUs),
    hostLatencyFrames: after.plugin.hostLatencyFrames,
    pluginLatencyFrames: after.plugin.latencyFrames,
    blocks: after.plugin.blocks,
  };
}
