// Audition without editing the song: a throwaway one track project that carries the sound,
// compiled to its own tiny blob and played by a separate preview engine in the worklet.
import { INSTRUMENT_SINE, type Project } from '../model';
import type { PackLoader } from '../sounds/packs';
import { WasmEditor } from '../wasm/editor';

/** Semitones above the root: a major triad, the plainest chord that shows a voice's body. */
export const PREVIEW_CHORD: readonly number[] = [0, 4, 7];

const PREVIEW_ID = 'a'.repeat(32);

export function previewNotes(root: number): number[] {
  return PREVIEW_CHORD.map((s) => Math.min(127, Math.max(0, root + s)));
}

/** One empty instrument track that plays `sound`; no regions, so only live notes sound. */
export function previewProject(sound: string): Project {
  return {
    id: PREVIEW_ID,
    name: 'preview',
    tempo_map: { tempos: { '0': 120000 }, meters: { '0': { numerator: 4, denominator: 4 } } },
    tracks: [
      {
        id: PREVIEW_ID.replace(/a/g, 'b'),
        name: 'Preview',
        kind: 'instrument',
        instrument: INSTRUMENT_SINE,
        sound,
        regions: {},
      },
    ],
  };
}

/** Compiles preview blobs with its own editor, so the open project is never touched. */
export class PreviewCompiler {
  private editor: WasmEditor | null = null;

  constructor(private readonly module: WebAssembly.Module) {}

  compile(sound: string, sampleRate: number): Uint8Array {
    this.editor ??= WasmEditor.fromModule(this.module, 'preview');
    const r = this.editor.load(previewProject(sound));
    if (r.error) throw new Error(r.error.message);
    return this.editor.engineBlob(sampleRate);
  }

  /** Prepares `sound` in the preview editor when it is a sampled sound; builtin sounds need nothing. */
  prepare(packs: PackLoader, sound: string): Promise<boolean> {
    this.editor ??= WasmEditor.fromModule(this.module, 'preview');
    return packs.ensure(this.editor, sound);
  }

  dispose(): void {
    this.editor?.free();
    this.editor = null;
  }
}
