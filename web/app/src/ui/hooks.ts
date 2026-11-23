import { createContext, useContext, useEffect, useState, useSyncExternalStore } from 'react';
import type { AudioStatus } from '../audio/audioEngine';
import type { Session } from '../session';
import type { Snapshot } from '../state/store';

export const SessionContext = createContext<Session | null>(null);

export function useSession(): Session {
  const s = useContext(SessionContext);
  if (!s) throw new Error('SessionContext is missing');
  return s;
}

export function useSnapshot(): Snapshot {
  const { store } = useSession();
  return useSyncExternalStore(store.subscribe, store.getSnapshot);
}

/** Audio status copy refreshed at a calm rate; the canvases read the live object themselves. */
export function useAudioStatus(intervalMs: number): AudioStatus {
  const { audio } = useSession();
  const [status, setStatus] = useState<AudioStatus>({ ...audio.status });
  useEffect(() => {
    const read = (): void => setStatus({ ...audio.status });
    const off = audio.subscribe(read);
    const timer = setInterval(read, intervalMs);
    return () => {
      off();
      clearInterval(timer);
    };
  }, [audio, intervalMs]);
  return status;
}
