import { useState } from 'react';

/**
 * Range input that shows a local draft while it is dragged and sends one command on release
 * (pointer up or key up), so a drag is one undo step. Double click resets to the neutral value.
 */
export function MixSlider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  neutral: number;
  text(v: number): string;
  onCommit(v: number): Promise<void>;
  className: string;
}) {
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? props.value;
  const commit = (v: number): void => {
    if (v === props.value) {
      setDraft(null);
      return;
    }
    setDraft(v);
    void props.onCommit(v).finally(() => setDraft(null));
  };
  return (
    <input
      type="range"
      aria-label={props.label}
      aria-valuetext={props.text(shown)}
      title={`${props.label}: ${props.text(shown)}`}
      className={props.className}
      min={props.min}
      max={props.max}
      step={props.step}
      value={shown}
      onChange={(e) => setDraft(Number(e.target.value))}
      onPointerUp={() => draft !== null && commit(draft)}
      onKeyUp={() => draft !== null && commit(draft)}
      onBlur={() => draft !== null && commit(draft)}
      onDoubleClick={() => commit(props.neutral)}
    />
  );
}
