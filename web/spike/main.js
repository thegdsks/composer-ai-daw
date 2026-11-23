import { WasmEditor } from './editor.mjs';

const statusEl = document.getElementById('status');
const startBtn = document.getElementById('start');
const onBtn = document.getElementById('note-on');
const offBtn = document.getElementById('note-off');
const addBtn = document.getElementById('add-note');
const editorEl = document.getElementById('editor-status');

// Latest status is also on window so automation can read it without parsing the page.
window.spikeStatus = null;
window.spikeError = null;
window.spikeEditor = null;

function show(msg) {
  window.spikeStatus = msg;
  statusEl.textContent = JSON.stringify(msg, null, 2);
}

startBtn.addEventListener('click', async () => {
  startBtn.disabled = true;
  try {
    const ctx = new AudioContext({ latencyHint: 'interactive' });
    await ctx.resume();
    const wasm = await (await fetch('composer_wasm.wasm')).arrayBuffer();
    await ctx.audioWorklet.addModule('worklet.js');

    // A shared counter tests whether the worklet sees main thread writes (Safari concern).
    let sab = null;
    let sabBuffer = null;
    if (typeof SharedArrayBuffer !== 'undefined') {
      sabBuffer = new SharedArrayBuffer(4);
      sab = new Int32Array(sabBuffer);
      setInterval(() => Atomics.add(sab, 0, 1), 100);
    }

    const node = new AudioWorkletNode(ctx, 'composer-engine', {
      numberOfInputs: 0,
      outputChannelCount: [2],
      processorOptions: { wasm, sabBuffer },
    });
    node.connect(ctx.destination);
    node.onprocessorerror = (e) => {
      window.spikeError = String(e.message ?? 'processor error');
      statusEl.textContent = 'processor error';
    };
    node.port.onmessage = (e) => {
      if (e.data.type !== 'status') return;
      show({
        ...e.data,
        main: {
          crossOriginIsolated: self.crossOriginIsolated,
          sharedArrayBuffer: typeof SharedArrayBuffer !== 'undefined',
          sabCounterOnMain: sab ? Atomics.load(sab, 0) : null,
          sampleRate: ctx.sampleRate,
          baseLatency: ctx.baseLatency,
          outputLatency: ctx.outputLatency ?? null,
          state: ctx.state,
        },
      });
    };
    const send = (on) =>
      node.port.postMessage({ type: 'note', track: 0, pitch: 69, velocity: 100, on });
    onBtn.onclick = () => send(true);
    offBtn.onclick = () => send(false);
    setupEditor(wasm, ctx, node);
    onBtn.disabled = false;
    offBtn.disabled = false;
    window.spikeSendNote = (track, pitch, on) =>
      node.port.postMessage({ type: 'note', track, pitch, velocity: 100, on });
  } catch (err) {
    window.spikeError = String(err);
    statusEl.textContent = `error: ${err}`;
    startBtn.disabled = false;
  }
});

// Edits go through the wasm editor (no daemon); each edit recompiles the project and the
// worklet reloads it, so the loop plays what the editor holds.
function setupEditor(wasmBytes, ctx, node) {
  const ed = WasmEditor.fromBytes(wasmBytes, 'spike');
  const seed = [
    { command: 'add_track', id: 'a', name: 'lead', kind: 'instrument', index: null },
    { command: 'set_instrument', track: 'a', instrument: '2' },
    { command: 'add_region', id: 'b', track: 'a', start: 0, length: 3840, kind: 'midi' },
  ];
  for (const c of seed) ed.apply(c);
  let n = 0;
  const publish = (r, blobBytes) => {
    const info = {
      stateHash: ed.stateHash,
      undoDepth: r.result?.undo_depth ?? null,
      error: r.error?.data?.code ?? null,
      notes: n,
      blobBytes,
    };
    window.spikeEditor = info;
    editorEl.textContent = JSON.stringify(info, null, 2);
  };
  addBtn.onclick = () => {
    const r = ed.apply({
      command: 'add_note',
      id: (0x100 + n).toString(16),
      region: 'b',
      pitch: 60 + (n % 12),
      velocity: 100,
      start: (n % 4) * 960,
      duration: 480,
    });
    if (r.result) n++;
    const blob = ed.engineBlob(ctx.sampleRate);
    node.port.postMessage({ type: 'load', blob });
    publish(r, blob.length);
  };
  addBtn.disabled = false;
  publish({ result: { undo_depth: 3 } }, null);
}
