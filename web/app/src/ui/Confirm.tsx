import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { Dialog } from './Dialog';
import { ConfirmContext, type Confirm, type ConfirmOptions } from './shell';

/** Gives children `useConfirm()`: ask a yes or no question, get a promise for the answer. */
export function ConfirmHost({ children }: { children: ReactNode }) {
  const [options, setOptions] = useState<ConfirmOptions | null>(null);
  const settle = useRef<((yes: boolean) => void) | null>(null);

  const done = useCallback((yes: boolean): void => {
    const s = settle.current;
    settle.current = null;
    setOptions(null);
    s?.(yes);
  }, []);

  const confirm = useCallback<Confirm>(
    (o) =>
      new Promise<boolean>((resolve) => {
        settle.current?.(false);
        settle.current = resolve;
        setOptions(o);
      }),
    [],
  );

  // A question left open while the host goes away counts as no.
  useEffect(() => () => settle.current?.(false), []);

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      <Dialog
        open={options !== null}
        onClose={() => done(false)}
        title={options?.title ?? ''}
        testId="confirm-dialog"
        danger={options?.danger}
        primary={{ label: options?.confirmLabel ?? 'OK', onClick: () => done(true) }}
        secondary={{ label: options?.cancelLabel ?? 'Cancel', onClick: () => done(false) }}
      >
        <p className="m-0 text-sm">{options?.message}</p>
      </Dialog>
    </ConfirmContext.Provider>
  );
}
