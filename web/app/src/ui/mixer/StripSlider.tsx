import { useState, type KeyboardEvent } from 'react';

/**
 * Native range input for a mixer strip. It keeps a local draft while it moves and sends one
 * command on release (pointer up, key up or blur), so a gesture is one undo step. Home and a
 * double click reset to the neutral value; arrows, Page keys and End come from the browser.
 */
export function StripSlider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  neutral: number;
  vertical?: boolean;
  text(v: number): string;
  /** True when the draft already equals what the project holds, so nothing is sent. */
  same(draft: number): boolean;
  onCommit(v: number): Promise<void>;
  className: string;
  testId?: string;
}) {
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? props.value;
  const commit = (v: number): void => {
    if (props.same(v)) {
      setDraft(null);
      return;
    }
    setDraft(v);
    void props.onCommit(v).finally(() => setDraft(null));
  };
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>): void => {
    if (e.key !== 'Home') return;
    e.preventDefault();
    commit(props.neutral);
  };
  const release = (): void => {
    if (draft !== null) commit(draft);
  };
  return (
    <input
      type="range"
      aria-label={props.label}
      aria-valuetext={props.text(shown)}
      aria-orientation={props.vertical ? 'vertical' : 'horizontal'}
      title={`${props.label}: ${props.text(shown)}. Home or double click resets.`}
      data-testid={props.testId}
      className={`${props.className} ${props.vertical ? '[direction:rtl] [writing-mode:vertical-lr]' : ''}`}
      min={props.min}
      max={props.max}
      step={props.step}
      value={shown}
      onChange={(e) => setDraft(Number(e.target.value))}
      onKeyDown={onKeyDown}
      onPointerUp={release}
      onPointerCancel={release}
      onKeyUp={release}
      onBlur={release}
      onDoubleClick={() => commit(props.neutral)}
    />
  );
}
