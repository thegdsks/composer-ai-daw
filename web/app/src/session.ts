// Boot: pick the backend, load the wasm module, restore or seed the project, wire autosave
// and the audio engine. The UI receives a ready Session.
import { AudioEngine } from './audio/audioEngine';
import { PreviewCompiler } from './audio/preview';
import { PackLoader } from './sounds/packs';
import type { Backend, SoundCatalogue } from './backend/types';
import { DaemonBackend, WasmBlobCompiler, tokenFromHash } from './backend/daemonBackend';
import { WasmBackend } from './backend/wasmBackend';
import { config, gridChoices } from './config';
import { installDebug, type ComposerDebug } from './debug';
import { installE2e } from './e2eHooks';
import { starterCommands, stressCommands } from './fixtures';
import { newId } from './ids';
import { notesOf } from './model';
import { loadLocal, saveLocal } from './persistence';
import { Actions } from './state/actions';
import { TransportActions } from './state/transportActions';
import { RecordActions } from './state/recordActions';
import { CatchActions } from './state/catchActions';
import { plainError } from './state/errors';
import { Store, findRegion, projectEnd } from './state/store';
import { WasmEditor } from './wasm/editor';

export interface Session {
  store: Store;
  audio: AudioEngine;
  actions: Actions;
  transport: TransportActions;
  record: RecordActions;
  catcher: CatchActions;
  debug: ComposerDebug;
  backendKind: Backend['kind'];
  /** True when the project came from the last autosave, so the start sheet is not the first thing seen. */
  restored: boolean;
  /** Set when the daemon was requested but could not be used. */
  notice: string | null;
  /** Sound catalogue from the backend; null when it has none (the slot is hidden). */
  sounds: { catalogue: SoundCatalogue | null; error: string | null };
  setNotifier(fn: (message: string) => void): void;
  dispose(): void;
}

async function loadWasm(): Promise<{ bytes: ArrayBuffer; module: WebAssembly.Module }> {
  const res = await fetch(`${import.meta.env.BASE_URL}composer_wasm.wasm`);
  if (!res.ok) throw new Error(`could not load the sound engine (${res.status})`);
  const bytes = await res.arrayBuffer();
  return { bytes, module: await WebAssembly.compile(bytes) };
}

/** Applies fixture commands, then drops the undo history so Undo cannot erase the fixture. */
async function seed(store: Store, label: string, commands: Parameters<Store['group']>[1]): Promise<void> {
  await store.group(label, commands);
  if (store.backend.kind === 'wasm') {
    const s = await store.backend.state();
    if (s.ok) await store.load(s.value.project);
  }
}

/** Backends without a catalogue answer `unsupported`; that hides the slot, other errors show. */
export async function loadSounds(backend: Backend): Promise<Session['sounds']> {
  const r = await backend.sounds();
  if (r.ok) return { catalogue: r.value, error: null };
  return { catalogue: null, error: r.error.code === 'unsupported' ? null : plainError(r.error) };
}

export async function boot(search: string, hash: string): Promise<Session> {
  const params = new URLSearchParams(search);
  const { bytes, module } = await loadWasm();
  let notice: string | null = null;

  let backend: Backend | null = null;
  const daemonUrl = params.get('daemon');
  if (daemonUrl) {
    const token = tokenFromHash(hash);
    if (!token) notice = 'Daemon requested without a token in the URL hash; using the in-browser editor.';
    else {
      try {
        backend = await DaemonBackend.connect(daemonUrl, token, new WasmBlobCompiler(WasmEditor.fromModule(module, 'blob')));
      } catch (e) {
        notice = `Could not reach the daemon (${e instanceof Error ? e.message : 'error'}); using the in-browser editor.`;
      }
    }
  }
  const packs = new PackLoader(`${import.meta.env.BASE_URL}sounds-packs/`);
  backend ??= WasmBackend.create(module, 'untitled', packs);

  const store = new Store(backend);
  await store.refresh();
  const stress = params.get('stress');
  const persist = backend.kind === 'wasm' && stress === null;
  let wasRestored = false;

  if (stress !== null) {
    const count = Number(stress) > 0 ? Number(stress) : config.stressNotes;
    const dim = (key: string, fallback: number): number => (Number(params.get(key)) > 0 ? Number(params.get(key)) : fallback);
    const fx = stressCommands(count, newId, config.stressChunk, dim('tracks', config.stressTracks), dim('regions', config.stressRegions));
    await store.group('Stress setup', fx.setup);
    for (const g of fx.groups) await store.group('Stress notes', g);
  } else if (backend.kind === 'wasm') {
    const saved = loadLocal();
    const loaded = saved ? await store.load(saved) : null;
    const usable = loaded?.ok === true && store.getSnapshot().project.tracks.length > 0;
    wasRestored = usable;
    if (!usable) await seed(store, 'Starter project', starterCommands(newId));
  } else if (store.getSnapshot().project.tracks.length === 0) {
    await seed(store, 'Starter project', starterCommands(newId));
  }
  await store.refresh();

  const sounds = await loadSounds(backend);
  const previewCompiler = new PreviewCompiler(module);
  const audio = new AudioEngine(
    bytes,
    (sr) => backend.engineBlob(sr),
    async (sound, sr) => {
      await previewCompiler.prepare(packs, sound);
      return previewCompiler.compile(sound, sr);
    },
  );
  installE2e(store, audio);
  const debug = installDebug({
    audio: () => audio.status,
    project: () => {
      const snap = store.getSnapshot();
      let regions = 0;
      let notes = 0;
      for (const t of snap.project.tracks) {
        for (const r of Object.values(t.regions)) {
          regions++;
          notes += notesOf(r).length;
        }
      }
      return {
        tracks: snap.project.tracks.length,
        regions,
        notes,
        hash: snap.stateHash,
        backend: backend.kind,
        undoDepth: snap.undoDepth,
        selectedNotes: snap.selectedNotes.size,
        /** Start ticks of the open clip's notes, sorted, for e2e checks of paste placement. */
        openNoteStarts: (() => {
          const open = findRegion(snap.project, snap.selectedRegion);
          return open ? notesOf(open.region).map((n) => n.start).sort((a, b) => a - b) : [];
        })(),
        /** Start and pitch of the open clip's notes, for e2e checks of chords. */
        openNotes: (() => {
          const open = findRegion(snap.project, snap.selectedRegion);
          return open ? notesOf(open.region).map((n) => ({ start: n.start, pitch: n.pitch })) : [];
        })(),
        sounds: snap.project.tracks.map((t) => t.sound ?? null),
        timeline: {
          markers: snap.project.markers ?? {},
          tempos: snap.project.tempo_map.tempos,
          loops: snap.project.tracks.flatMap((t) => Object.values(t.regions)).filter((r) => (r.loop_length ?? 0) > 0).length,
        },
        transport: snap.transport,
        record: { phase: record.getSnapshot().phase, lastStarts: record.getSnapshot().lastStarts },
        caught: catcher.getSnapshot().count,
      };
    },
  });

  let notify: (m: string) => void = () => undefined;
  const actions = new Actions(store, module, debug, (m) => notify(m));
  const transport = new TransportActions(store, audio, (m) => notify(m));
  const record = new RecordActions(store, audio, actions, transport, (m) => notify(m));
  const catcher = new CatchActions(
    store,
    {
      now: () => performance.now(),
      playTick: () => audio.playTick,
      grid: () => {
        const r = record.getSnapshot();
        return r.tidy ? (gridChoices().find((c) => c.id === r.gridId)?.ticks ?? null) : null;
      },
      limits: { windowMs: config.catchSeconds * 1000, maxNotes: config.catchMaxNotes },
    },
    (m) => notify(m),
  );

  // The worklet follows tempo, loop and metronome from the session state; none of it is project data.
  const pushTransport = (): void => {
    const snap = store.getSnapshot();
    audio.setTransportInputs({ map: snap.project.tempo_map, prefs: snap.transport, endTick: projectEnd(snap.project) });
  };
  store.subscribe(pushTransport);
  pushTransport();
  if (backend.kind === 'wasm') {
    // A fresh page starts with the default count in and the loop set to the first clip, switched off.
    await store.setTransport({ countInBars: config.countInBars });
    if (stress === null) await transport.loopSelection(false);
  }

  // Edits rebuild the engine blob (debounced) and, for the in-page editor, autosave.
  let lastHash = store.getSnapshot().stateHash;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  store.subscribe(() => {
    const snap = store.getSnapshot();
    if (snap.stateHash === lastHash) return;
    lastHash = snap.stateHash;
    audio.scheduleReload();
    if (!persist) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => saveLocal(store.getSnapshot().project), config.autosaveDebounceMs);
  });

  // The first gesture of any kind unlocks audio.
  const unlock = (): void => {
    void audio.ensure();
    window.removeEventListener('pointerdown', unlock, true);
    window.removeEventListener('keydown', unlock, true);
  };
  window.addEventListener('pointerdown', unlock, true);
  window.addEventListener('keydown', unlock, true);

  return {
    store,
    audio,
    actions,
    transport,
    record,
    catcher,
    debug,
    backendKind: backend.kind,
    notice,
    restored: wasRestored,
    sounds,
    setNotifier: (fn) => {
      notify = fn;
    },
    dispose: () => {
      if (saveTimer) clearTimeout(saveTimer);
      window.removeEventListener('pointerdown', unlock, true);
      window.removeEventListener('keydown', unlock, true);
      record.dispose();
      catcher.dispose();
      audio.dispose();
      previewCompiler.dispose();
      store.dispose();
    },
  };
}
