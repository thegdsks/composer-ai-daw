// With ?e2e a driver stands in for an AI client: it can post a proposal and read the history
// source tags. Results land in plain fields so a script can poll them without awaiting a promise.
import type { Command } from './model';
import type { AudioEngine } from './audio/audioEngine';
import type { Store } from './state/store';

export interface ComposerE2e {
  post(label: string, commands: Command[], note?: string, author?: string): void;
  loadSources(): void;
  setPluginParam(param: number, value: number): void;
  lastPost: string | null;
  sources: string[];
}

export function installE2e(store: Store, audio: AudioEngine): void {
  if (!new URLSearchParams(location.search).has('e2e')) return;
  const hooks: ComposerE2e = {
    lastPost: null,
    sources: [],
    post(label, commands, note, author) {
      hooks.lastPost = null;
      void store.backend.proposalPost?.({ label, commands, note, author }).then((r) => {
        hooks.lastPost = r.ok ? `ok:${r.value.id ?? ''}` : `error:${r.error.code}`;
      });
    },
    setPluginParam(param, value) {
      audio.setPluginParam(param, value);
    },
    loadSources() {
      void store.backend.history(0, 200).then((r) => {
        hooks.sources = r.ok ? r.value.entries.map((e) => `${e.source}:${e.label}`) : [];
      });
    },
  };
  (window as unknown as { composerE2e: ComposerE2e }).composerE2e = hooks;
}
