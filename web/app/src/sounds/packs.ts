// Sampled sounds (SF2 packs fetched by sounds/fetch.sh, D37): the list comes from a small
// index served next to the files, the bytes are fetched once on first use and handed to the
// wasm editor. A page without the packs simply lists no sampled sounds.
import type { SoundEntry } from '../backend/types';

/** Anything that can take a sample bank: the wasm editor and the preview editor. */
export interface BankSink {
  loadSoundBank(id: string, bytes: Uint8Array): number;
}

type Get = (url: string) => Promise<Response>;

export class PackLoader {
  private index: Promise<SoundEntry[]> | null = null;
  private readonly bytes = new Map<string, Promise<Uint8Array | null>>();
  private readonly loaded = new WeakMap<BankSink, Set<string>>();

  constructor(
    private readonly base: string,
    private readonly get: Get = (u) => fetch(u),
  ) {}

  /** Wired sampled sounds, empty when the packs are not served. */
  entries(): Promise<SoundEntry[]> {
    this.index ??= this.get(`${this.base}index.json`)
      .then((r) => (r.ok ? (r.json() as Promise<SoundEntry[]>) : []))
      .catch(() => []);
    return this.index;
  }

  private fetchBytes(file: string): Promise<Uint8Array | null> {
    let p = this.bytes.get(file);
    if (!p) {
      p = this.get(`${this.base}${file}`)
        .then(async (r) => (r.ok ? new Uint8Array(await r.arrayBuffer()) : null))
        .catch(() => null);
      this.bytes.set(file, p);
    }
    return p;
  }

  /** True when `id` is a sampled sound and its bank is now loaded in `sink`. */
  async ensure(sink: BankSink, id: string): Promise<boolean> {
    let done = this.loaded.get(sink);
    if (done?.has(id)) return true;
    const entry = (await this.entries()).find((e) => e.id === id);
    if (!entry?.file) return false;
    const data = await this.fetchBytes(entry.file);
    if (!data || sink.loadSoundBank(id, data) !== 0) return false;
    done ??= new Set();
    done.add(id);
    this.loaded.set(sink, done);
    return true;
  }
}
