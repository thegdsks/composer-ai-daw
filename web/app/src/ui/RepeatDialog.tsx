import { useEffect, useRef, useState, type FormEvent } from 'react';
import { config } from '../config';
import { btn, btnPrimary, field } from './classes';
import { useSession } from './hooks';

interface Ask {
  what: string;
  resolve(n: number | null): void;
}

/** Asks how many times to repeat the selection (Ctrl Shift D). The answer goes back to Actions. */
export function RepeatDialog() {
  const { actions } = useSession();
  const ref = useRef<HTMLDialogElement>(null);
  const [ask, setAsk] = useState<Ask | null>(null);
  const [value, setValue] = useState('1');
  const [error, setError] = useState('');

  useEffect(() => {
    actions.setRepeatPrompt((what) => new Promise<number | null>((resolve) => setAsk({ what, resolve })));
    return () => actions.setRepeatPrompt(null);
  }, [actions]);

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (ask && !d.open) {
      setValue('1');
      setError('');
      d.showModal();
    }
  }, [ask]);

  const finish = (n: number | null): void => {
    ask?.resolve(n);
    setAsk(null);
    const d = ref.current;
    if (d?.open) d.close();
  };

  const submit = (e: FormEvent): void => {
    e.preventDefault();
    const n = Number(value);
    if (!Number.isInteger(n) || n < 1 || n > config.maxRepeat) {
      setError(`Enter a whole number from 1 to ${config.maxRepeat}.`);
      return;
    }
    finish(n);
  };

  return (
    <dialog
      ref={ref}
      aria-labelledby="repeat-title"
      className="m-auto w-80 max-w-[92vw] overscroll-contain rounded-lg border border-line bg-panel p-4 text-ink backdrop:overflow-hidden backdrop:overscroll-contain backdrop:bg-black/50"
      onCancel={() => finish(null)}
    >
      <form onSubmit={submit} className="flex flex-col gap-3">
        <h2 id="repeat-title" className="text-base font-semibold">
          Repeat the selected {ask?.what ?? 'clips'}
        </h2>
        <label className="flex flex-col gap-1 text-sm text-ink-dim">
          How many more times
          <input
            autoFocus
            type="number"
            inputMode="numeric"
            min={1}
            max={config.maxRepeat}
            step={1}
            className={`${field} text-ink`}
            value={value}
            aria-describedby="repeat-error"
            onChange={(e) => setValue(e.target.value)}
          />
        </label>
        <p id="repeat-error" role="alert" className="m-0 min-h-5 text-sm text-danger">
          {error}
        </p>
        <div className="flex justify-end gap-2">
          <button type="button" className={btn} onClick={() => finish(null)}>
            Cancel
          </button>
          <button type="submit" className={btnPrimary}>
            Repeat
          </button>
        </div>
      </form>
    </dialog>
  );
}
