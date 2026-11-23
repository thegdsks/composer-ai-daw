import { useSyncExternalStore } from 'react';
import { XIcon } from '../icons';
import { btn, iconBtn } from './classes';
import { useToasts } from './shell';

/** Bottom stack of toasts. Each is a polite status; the optional button (Undo) comes first. */
export function Toasts() {
  const queue = useToasts();
  const toasts = useSyncExternalStore(queue.subscribe, queue.getSnapshot);
  if (toasts.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-10 left-1/2 z-30 flex -translate-x-1/2 flex-col items-center gap-2" data-testid="toast-stack">
      {toasts.map((t) => (
        <div
          key={t.id}
          role="status"
          data-testid="toast"
          className="pointer-events-auto flex items-center gap-3 rounded-md border border-line bg-panel px-3 py-2 text-sm text-ink shadow-lg"
        >
          <span>{t.text}</span>
          {t.action && (
            <button type="button" className={btn} onClick={() => void queue.act(t.id)}>
              {t.action.label}
            </button>
          )}
          <button type="button" className={iconBtn} aria-label="Dismiss" title="Dismiss" onClick={() => queue.dismiss(t.id)}>
            <XIcon size={16} />
          </button>
        </div>
      ))}
    </div>
  );
}
