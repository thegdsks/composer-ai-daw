import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { btn, btnPrimary } from './classes';
import { focusReturn } from './focusReturn';

export interface DialogButton {
  label: string;
  onClick?(): void;
  disabled?: boolean;
}

const btnDanger =
  'inline-flex h-9 items-center justify-center gap-1.5 rounded-md border border-accent bg-accent px-3 text-sm font-semibold text-accent-ink ' +
  'hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50 motion-safe:transition-opacity';

const pageFocus = {
  get activeElement(): HTMLElement | null {
    return document.activeElement instanceof HTMLElement ? document.activeElement : null;
  },
};

const WIDTH = { sm: 'w-96', lg: 'w-[30rem]' } as const;

/**
 * The one dialog every sheet uses: a native modal dialog (focus stays inside, Escape closes),
 * a headline, a body, one primary and one secondary button. Focus goes back to the opener on close.
 * Put data-autofocus on a field that should take focus first; otherwise the primary button does.
 */
export function Dialog(props: {
  open: boolean;
  onClose(): void;
  title: string;
  children?: ReactNode;
  primary?: DialogButton;
  /** Defaults to Cancel, which closes. Pass false for a one button dialog. */
  secondary?: DialogButton | false;
  /** Turns the body into a form; the primary button submits it. */
  onSubmit?(): void;
  width?: keyof typeof WIDTH;
  testId?: string;
  kind?: string;
  /** Marks the primary action as destructive. */
  danger?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const latest = useRef(props);
  useEffect(() => {
    latest.current = props;
  });
  const [focus] = useState(() => focusReturn(pageFocus));
  const { open } = props;

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) {
      focus.capture();
      d.showModal();
      const first = d.querySelector<HTMLElement>('[data-autofocus]') ?? d.querySelector<HTMLElement>('[data-dialog-primary]');
      first?.focus();
    } else if (!open && d.open) {
      d.close();
      focus.restore();
    }
  }, [open, focus]);

  // Escape and the native close both land here; the opener gets focus back either way.
  const onNativeClose = (): void => {
    focus.restore();
    if (latest.current.open) latest.current.onClose();
  };

  const secondary = props.secondary === undefined ? { label: 'Cancel' } : props.secondary;
  const primary = props.primary;
  const submit = (e: FormEvent): void => {
    e.preventDefault();
    props.onSubmit?.();
  };

  const body = (
    <>
      <h2 id={titleId} className="text-base font-semibold">
        {props.title}
      </h2>
      {props.children}
      {(primary || secondary) && (
        <div className="flex justify-end gap-2">
          {secondary && (
            <button type="button" className={btn} disabled={secondary.disabled} onClick={secondary.onClick ?? props.onClose}>
              {secondary.label}
            </button>
          )}
          {primary && (
            <button
              type={props.onSubmit ? 'submit' : 'button'}
              data-dialog-primary=""
              className={props.danger ? btnDanger : btnPrimary}
              disabled={primary.disabled}
              onClick={props.onSubmit ? undefined : (primary.onClick ?? props.onClose)}
            >
              {primary.label}
            </button>
          )}
        </div>
      )}
    </>
  );

  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      data-testid={props.testId}
      data-kind={props.kind}
      className={`m-auto ${WIDTH[props.width ?? 'sm']} max-w-[92vw] overscroll-contain rounded-lg border border-line bg-panel p-4 text-ink backdrop:overflow-hidden backdrop:overscroll-contain backdrop:bg-black/50`}
      onClose={onNativeClose}
    >
      {props.onSubmit ? (
        <form onSubmit={submit} className="flex flex-col gap-3">
          {body}
        </form>
      ) : (
        <div className="flex flex-col gap-3">{body}</div>
      )}
    </dialog>
  );
}
