import { useEffect, useState, useSyncExternalStore } from 'react';
import { config } from '../config';
import { getSaveStatus, subscribeSaveStatus } from '../persistence';
import { useAudioStatus, useSession, useSnapshot } from './hooks';

const STATE_TEXT: Record<string, string> = {
  off: 'Audio starts when you press a key or click',
  starting: 'Starting audio',
  running: 'Audio running',
  suspended: 'Audio paused by the browser, press Play',
  error: 'Audio error',
};

const NOTICE_KEY = 'composer.storage-notice.v1';

function clock(d: Date): string {
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function megabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Browser storage in use, re-read after each autosave. Null when the browser cannot say. */
function useStorageUse(trigger: Date | null): string | null {
  const [use, setUse] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const estimate = typeof navigator !== 'undefined' ? navigator.storage?.estimate : undefined;
    if (!estimate) return;
    void navigator.storage.estimate().then((e) => {
      if (live && e.usage !== undefined) setUse(`${megabytes(e.usage)}${e.quota ? ` of ${megabytes(e.quota)}` : ''}`);
    });
    return () => {
      live = false;
    };
  }, [trigger]);
  return use;
}

export function StatusBar(props: { message: string | null; octave: number }) {
  const { backendKind, notice, audio, record } = useSession();
  const save = useSyncExternalStore(subscribeSaveStatus, getSaveStatus, getSaveStatus);
  const storage = useStorageUse(save.at);
  const [once, setOnce] = useState<string | null>(null);
  const snap = useSnapshot();
  const keys = useSyncExternalStore(record.subscribe, record.getSnapshot, record.getSnapshot);
  const a = useAudioStatus(config.meterRefreshMs);
  // The first successful autosave explains, once, where the work lives.
  useEffect(() => {
    if (backendKind !== 'wasm') return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const off = subscribeSaveStatus(() => {
      if (!getSaveStatus().at) return;
      try {
        if (localStorage.getItem(NOTICE_KEY)) return;
        localStorage.setItem(NOTICE_KEY, '1');
      } catch {
        return;
      }
      setOnce('Your work is kept in this browser only. Use Save to file to keep a copy.');
      timer = setTimeout(() => setOnce(null), config.toastMs);
    });
    return () => {
      off();
      if (timer) clearTimeout(timer);
    };
  }, [backendKind]);
  const latency = audio.latencyMs;
  const peak = a.state === 'running' ? a.peak : 0;
  return (
    <footer className="flex flex-wrap items-center gap-x-4 gap-y-1 min-h-7 border-t border-line bg-panel px-3 py-1 text-[13px] text-ink-dim">
      <span data-testid="audio-state" data-state={a.state} data-transport={a.transport}>
        {STATE_TEXT[a.state]}
        {a.state === 'error' && a.error ? `: ${a.error}` : ''}
        {a.sampleRate > 0 ? ` (${a.sampleRate} Hz${latency === null ? '' : `, ${latency} ms delay`})` : ''}
      </span>
      <label className="flex items-center gap-1.5">
        Level
        <meter
          className="h-2 w-28"
          min={0}
          max={1}
          low={0.7}
          high={0.95}
          optimum={0.4}
          value={Math.min(1, peak)}
          aria-label="Output level"
          data-testid="level-meter"
          data-peak={peak.toFixed(4)}
          data-max-peak={a.maxPeak.toFixed(4)}
        />
        <span className="w-10 font-mono">{peak.toFixed(2)}</span>
      </label>
      <span>
        {keys.pianoKeys || keys.phase !== 'idle'
          ? `Keys: A to ; play notes, Z and X shift the octave (${props.octave >= 0 ? '+' : ''}${props.octave})`
          : 'Keys are off. Press K to play with the computer keyboard.'}
      </span>
      {backendKind === 'daemon' ? (
        <span>Connected to composerd</span>
      ) : (
        <span data-testid="save-state" data-failed={save.failed}>
          {save.failed
            ? 'Could not save in this browser. Use Save to file now.'
            : save.at
              ? `Saved in this browser at ${clock(save.at)}`
              : 'Saves in this browser as you edit'}
          {storage ? ` (storage ${storage})` : ''}
        </span>
      )}
      <span className="ml-auto text-ink" role="status" aria-live="polite">
        {snap.error ?? props.message ?? once ?? notice ?? ''}
      </span>
    </footer>
  );
}
