import { useId, useRef, type ReactNode } from 'react';
import { MoreIcon } from '../icons';
import { btn, compact } from './classes';

/**
 * Rarely used top bar controls live here so the bar stays on one row. The panel is a native
 * popover: Escape and an outside click close it, and focus returns to the button.
 */
export function MoreMenu(props: { children: ReactNode; waiting?: number }) {
  const id = useId();
  const panel = useRef<HTMLDivElement>(null);
  return (
    <>
      <button
        type="button"
        className={`${compact(btn)} relative`}
        popoverTarget={id}
        aria-haspopup="dialog"
        aria-label={props.waiting ? `More controls, ${props.waiting} suggestions waiting` : 'More controls'}
        data-testid="more-button"
        title="More: time signature, tap tempo, count in, recording timing and MIDI"
      >
        <MoreIcon size={18} />
        <span className="max-[2000px]:sr-only">More</span>
        {props.waiting ? (
          <span
            data-testid="assist-badge"
            aria-hidden="true"
            className="absolute -right-0.5 -top-0.5 flex min-w-4 items-center justify-center rounded-full bg-accent px-1 text-[10px] font-semibold leading-4 text-accent-ink"
          >
            {props.waiting}
          </span>
        ) : null}
      </button>
      <div
        ref={panel}
        id={id}
        popover="auto"
        role="dialog"
        aria-label="More controls"
        data-testid="more-panel"
        onToggle={(e) => {
          if (e.nativeEvent instanceof ToggleEvent && e.nativeEvent.newState === 'open') {
            panel.current?.querySelector<HTMLElement>('button, select, input')?.focus();
          }
        }}
        className="fixed inset-auto right-3 top-14 m-0 w-72 supports-[top:anchor(bottom)]:right-[anchor(right)] supports-[top:anchor(bottom)]:top-[calc(anchor(bottom)+0.25rem)] flex-col [&:popover-open]:flex gap-2 rounded-md border border-line bg-panel p-3 text-ink shadow-md"
      >
        {props.children}
      </div>
    </>
  );
}
