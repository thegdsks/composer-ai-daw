// Fresh profile: the start sheet shows over the starter; one click on Play the starter must be audible within the env budget.
import { assert, envNum } from './lib.mjs';

export default async function smoke({ page, url }) {
  const budget = envNum('VITE_E2E_FIRST_SOUND_MS', 10000);
  page.open(`${url}?e2e=1`);
  await page.waitDebug((d) => d.project && d.project.tracks > 0, 'starter project');
  page.click('Play the starter');
  const d = await page.waitDebug((x) => x.firstSoundMs !== null, 'first sound', budget);
  assert(d.firstSoundMs <= budget, `firstSoundMs ${d.firstSoundMs} over budget ${budget}`);
  assert(d.audio.state !== 'error', `audio error: ${d.audio.error}`);
  return {
    firstSoundMs: Math.round(d.firstSoundMs),
    gestureToSoundMs: Math.round(d.firstSoundMs - d.firstGestureMs),
    actionsToFirstSound: d.actionsToFirstSound,
    tracks: d.project.tracks,
    notes: d.project.notes,
    undoDepth: d.undoDepth,
    commandLog: d.commandLog.length,
    sampleRate: d.audio.sampleRate,
  };
}
