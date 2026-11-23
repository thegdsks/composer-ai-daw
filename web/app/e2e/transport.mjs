// U10 exit: a ruler click at bar 3 moves the playhead there; a loop over bars 1 to 2 wraps (the
// playhead never passes bar 3 over 10 s); the metronome and count in click; Follow pages the view.
// Run: node e2e/run.mjs transport
import { assert, envNum } from './lib.mjs';
import { announcer, boot, ev, press, sleep, state } from './clip-lib.mjs';

const BAR = 3840;
const PLAY_MS = envNum('E2E_LOOP_MS', 10000);
const audio = (page) => page.debug().audio;
const prefs = (page) => page.debug().project.transport;

async function waitAudio(page, pick, what, timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const a = audio(page);
    if (pick(a)) return a;
    assert(Date.now() < deadline, `timed out waiting for ${what}; audio ${JSON.stringify(a)}`);
    await sleep(100);
  }
}

async function waitPrefs(page, pick, what) {
  const deadline = Date.now() + 5000;
  for (;;) {
    const t = prefs(page);
    if (pick(t)) return t;
    assert(Date.now() < deadline, `timed out waiting for ${what}; transport ${JSON.stringify(t)}`);
    await sleep(100);
  }
}

function rulerPoint(page, bar) {
  const { arr } = state(page);
  return {
    x: Math.round(arr.canvas.x + (bar * BAR - arr.scrollTick) * arr.pxPerTick),
    y: Math.round(arr.canvas.y + arr.rulerHeight / 2),
  };
}

function click(page, p) {
  page.run(['mouse', 'move', String(p.x), String(p.y)]);
  page.run(['mouse', 'down']);
  page.run(['mouse', 'up']);
}

export default async function transport({ page, url }) {
  await boot(page, url);
  const out = {};
  const bpm = Number(ev(page, `document.querySelector('[aria-label="Tempo in beats per minute"]').value`));
  const barSeconds = (4 * 60) / bpm;

  // 1. Ruler click at bar 3 puts the playhead there (stopped stays stopped).
  click(page, rulerPoint(page, 2));
  const a1 = await waitAudio(page, (a) => a.seeks >= 1 && a.sampleRate > 0, 'the seek to bar 3');
  const expected = Math.round(2 * barSeconds * a1.sampleRate);
  assert(Math.abs(a1.positionFrames - expected) <= 1, `ruler click: playhead at ${a1.positionFrames} frames, wanted ${expected}`);
  assert(a1.transport !== 'playing', 'a ruler click while stopped must not start playing');
  out.seek = { bar: 3, frames: a1.positionFrames, expected, seeks: a1.seeks };

  // 2. Dragging the brace end handle resizes the loop and turns it on.
  // The starter loop is a 4 bar brace (off); dragging its end handle from bar 5 to bar 3 leaves bars 1 and 2.
  const from = rulerPoint(page, 4);
  const to = rulerPoint(page, 2);
  page.run(['mouse', 'move', String(from.x), String(from.y)]);
  page.run(['mouse', 'down']);
  for (let i = 1; i <= 6; i++) page.run(['mouse', 'move', String(Math.round(from.x + ((to.x - from.x) * i) / 6)), String(to.y)]);
  page.run(['mouse', 'up']);
  const t1 = await waitPrefs(page, (t) => t.loop.on && t.loop.start === 0 && t.loop.end === 2 * BAR, 'loop over bars 1 and 2');
  out.loop = t1.loop;
  await announcer(page, (t) => /Loop on, bars 1 to 2/.test(t), '"Loop on, bars 1 to 2"');

  // 3. Play for PLAY_MS: the playhead must never reach bar 3 and must wrap more than once.
  press(page, 'Space');
  const startedAt = Date.now();
  await waitAudio(page, (a) => a.transport === 'playing', 'playing');
  const endFrames = Math.round(2 * barSeconds * a1.sampleRate);
  let maxFrames = 0;
  let samples = 0;
  while (Date.now() - startedAt < PLAY_MS) {
    const a = audio(page);
    maxFrames = Math.max(maxFrames, a.positionFrames);
    samples++;
    assert(a.positionFrames < endFrames, `playhead reached ${a.positionFrames} frames, loop ends at ${endFrames}`);
    await sleep(150);
  }
  const a2 = audio(page);
  const wanted = Math.floor(PLAY_MS / 1000 / (2 * barSeconds)) - 1;
  assert(a2.loopWraps >= Math.max(1, wanted), `only ${a2.loopWraps} wraps in ${PLAY_MS} ms (wanted at least ${Math.max(1, wanted)})`);
  assert(a2.state !== 'error', `audio error: ${a2.error}`);
  out.wrap = { samples, maxFrames, endFrames, wraps: a2.loopWraps, ms: PLAY_MS };

  // 4. Metronome: M on the canvas toggles it and clicks start counting while playing.
  press(page, 'm');
  await waitPrefs(page, (t) => t.metronome.on, 'metronome on');
  const c0 = audio(page).clicks;
  await sleep(2500);
  const c1 = audio(page).clicks;
  assert(c1 - c0 >= 3, `metronome clicked ${c1 - c0} times in 2.5 s`);
  out.metronome = { clicks: c1 - c0 };

  // 5. Count in: with 1 bar, Play clicks first and the playhead holds still.
  press(page, 'Space');
  await waitAudio(page, (a) => a.transport === 'paused', 'paused');
  press(page, 'Home');
  await waitAudio(page, (a) => a.positionFrames === 0, 'playhead at start');
  const countIn = ev(page, `document.querySelector('[aria-label="Count in before Play"]').value`);
  assert(countIn === '1', `count in defaults to 1 bar, found ${countIn}`);
  const cBefore = audio(page).clicks;
  press(page, 'Space');
  const counting = await waitAudio(page, (a) => a.counting, 'the count in');
  let peak = 0;
  const holdStart = Date.now();
  while (Date.now() - holdStart < barSeconds * 1000 + 2000) {
    // One read decides both: a second read can land after the count in ended (slow machine).
    const a = audio(page);
    if (!a.counting) break;
    peak = Math.max(peak, a.clickPeak);
    assert(a.positionFrames === 0, `playhead moved to ${a.positionFrames} during the count in`);
    await sleep(100);
  }
  const after = await waitAudio(page, (a) => !a.counting && a.positionFrames > 0, 'the song to start after the count in');
  assert(after.clicks - cBefore >= 4, `count in gave ${after.clicks - cBefore} clicks, wanted at least 4`);
  assert(peak > 0.05, `count in was silent (peak ${peak})`);
  out.countIn = { clicks: after.clicks - cBefore, peak: Math.round(peak * 1000) / 1000 };

  // 6. Loop off, then Follow: zoom in, play to the edge, the view pages; Follow off holds it.
  press(page, 'l');
  await waitPrefs(page, (t) => !t.loop.on, 'loop off');
  press(page, 'Space');
  await waitAudio(page, (a) => a.transport === 'paused', 'paused again');
  for (let i = 0; i < 4; i++) press(page, '+');
  press(page, 'Home');
  await waitAudio(page, (a) => a.positionFrames === 0, 'start before follow');
  const view0 = state(page).arr;
  assert(view0.scrollTick === 0, `view should sit at the start, scrollTick ${view0.scrollTick}`);
  press(page, 'm');
  await waitPrefs(page, (t) => !t.metronome.on, 'metronome off');
  press(page, 'Space');
  const spanTicks = view0.canvas.width / view0.pxPerTick;
  const wait = (spanTicks / 960) * (60 / bpm) * 1000 + 2500;
  await sleep(Math.min(wait, 20000));
  const followed = state(page).arr.scrollTick;
  assert(followed > 0, `Follow on: the view stayed at scrollTick ${followed} while the playhead ran on`);
  out.follow = { spanTicks: Math.round(spanTicks), scrollTick: Math.round(followed) };

  if (audio(page).transport === 'playing') press(page, 'Space');
  page.click('Follow');
  await waitPrefs(page, (t) => !t.follow, 'follow off');
  ev(page, `(document.getElementById('arrangement-canvas').focus(), true)`);
  press(page, 'Home');
  await waitAudio(page, (a) => a.positionFrames === 0, 'start after follow off');
  const keep = state(page).arr.scrollTick;
  assert(keep === 0, `Home should scroll to 0, got ${keep}`);
  press(page, 'Space');
  await sleep(Math.min(wait, 20000));
  const held = state(page).arr.scrollTick;
  assert(held === 0, `Follow off: the view moved to ${held}`);
  out.followOff = { scrollTick: held };
  return out;
}
