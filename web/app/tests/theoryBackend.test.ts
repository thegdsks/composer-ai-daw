// D35: theory reads through both backends. The daemon side is an in-process stand in that
// records requests and answers from the same wasm exports, so shapes and wire params are checked.
import { describe, expect, it } from 'vitest';
import { DaemonBackend, WasmBlobCompiler, type RpcSocket } from '../src/backend/daemonBackend';
import type { Backend, TheoryKey } from '../src/backend/types';
import { WasmEditor } from '../src/wasm/editor';
import { newBackend, wasmModule } from './helpers';

class TheoryDaemon implements RpcSocket {
  onmessage: ((data: string) => void) | null = null;
  onclose: (() => void) | null = null;
  readonly editor = WasmEditor.fromModule(wasmModule, 'daemon');
  readonly calls: { method: string; params: Record<string, unknown> }[] = [];

  send(data: string): void {
    const req = JSON.parse(data) as { id: number; method: string; params: Record<string, unknown> };
    this.calls.push({ method: req.method, params: req.params });
    queueMicrotask(() => {
      const { params } = req;
      const key = params.key as TheoryKey | undefined;
      const raw =
        req.method === 'theory.scale'
          ? this.editor.scaleJson(params)
          : req.method === 'theory.chords'
            ? this.editor.chordsJson(params)
            : this.editor.pitchJson(key ? { ...key, pitch: params.pitch } : params);
      this.onmessage?.(JSON.stringify({ jsonrpc: '2.0', id: req.id, ...raw }));
    });
  }

  close(): void {}
}

function daemon(): { fake: TheoryDaemon; b: Backend } {
  const fake = new TheoryDaemon();
  return { fake, b: new DaemonBackend(fake, new WasmBlobCompiler(WasmEditor.fromModule(wasmModule, 'blob'))) };
}

const dMinor: TheoryKey = { root: 2, scale: 'minor' };

describe('theory reads', () => {
  it('wasm backend returns scale, chords and pitch facts', async () => {
    const b = newBackend();
    const scale = await b.theoryScale!(dMinor);
    expect(scale).toEqual({
      ok: true,
      value: { root: 2, scale: 'minor', pitchClasses: [2, 4, 5, 7, 9, 10, 0], mask: expect.any(Number) },
    });
    const chords = await b.theoryChords!({ root: 0, scale: 'major' }, 3);
    if (!chords.ok) throw new Error('chords failed');
    expect(chords.value.rootName).toBe('C');
    expect(chords.value.octave).toBe(3);
    expect(chords.value.triads.map((c) => c.name)).toEqual(['C', 'Dm', 'Em', 'F', 'G', 'Am', 'Bdim']);
    expect(chords.value.sevenths[4]?.name).toBe('G7');
    const pitch = await b.theoryPitch!({ root: 0, scale: 'major' }, 61);
    expect(pitch).toEqual({ ok: true, value: { pitch: 61, inKey: false, degree: null, nearest: 60 } });
    b.dispose();
  });

  it('reports bad input as a coded error, never a throw', async () => {
    const b = newBackend();
    const r = await b.theoryPitch!({ root: 12, scale: 'major' }, 60);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.code).toBe('invalid_params');
    const o = await b.theoryChords!({ root: 0, scale: 'major' }, 99);
    expect(o.ok).toBe(false);
    b.dispose();
  });

  it('daemon backend sends the theory methods and matches the wasm backend', async () => {
    const { fake, b } = daemon();
    const w = newBackend();
    expect(await b.theoryScale!(dMinor)).toEqual(await w.theoryScale!(dMinor));
    expect(await b.theoryChords!(dMinor)).toEqual(await w.theoryChords!(dMinor));
    expect(await b.theoryChords!(dMinor, 2)).toEqual(await w.theoryChords!(dMinor, 2));
    expect(await b.theoryPitch!(dMinor, 70)).toEqual(await w.theoryPitch!(dMinor, 70));
    expect(fake.calls.map((c) => c.method)).toEqual([
      'theory.scale',
      'theory.chords',
      'theory.chords',
      'theory.pitch',
    ]);
    expect(fake.calls[0]?.params).toEqual({ root: 2, scale: 'minor' });
    expect(fake.calls[1]?.params).toEqual({ key: { root: 2, scale: 'minor' } });
    expect(fake.calls[2]?.params).toEqual({ key: { root: 2, scale: 'minor' }, octave: 2 });
    expect(fake.calls[3]?.params).toEqual({ key: { root: 2, scale: 'minor' }, pitch: 70 });
    b.dispose();
    w.dispose();
  });

  it('daemon backend passes coded errors through', async () => {
    const { b } = daemon();
    const r = await b.theoryScale!({ root: 40, scale: 'major' });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.code).toBe('invalid_params');
    b.dispose();
  });
});
