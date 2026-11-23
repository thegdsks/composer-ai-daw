import { useEffect, useState } from 'react';
import { config } from '../config';
import { useSession } from './hooks';

/** One polite live region. It speaks the plain step label of every committed command. */
export function Announcer() {
  const { store } = useSession();
  const [text, setText] = useState('');

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const off = store.onAnnounce((next) => {
      // Clearing first makes a repeated label (two notes in a row) get spoken again.
      setText('');
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => setText(next), config.announceGapMs);
    });
    return () => {
      off();
      if (timer) clearTimeout(timer);
    };
  }, [store]);

  return (
    <div className="sr-only" role="status" aria-live="polite" aria-atomic="true" data-testid="announcer">
      {text}
    </div>
  );
}
