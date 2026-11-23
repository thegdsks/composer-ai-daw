import { useEffect, useRef, useState, type KeyboardEvent } from 'react';

/** Text input that commits on Enter or blur and reverts on Escape. */
export function CommitText(props: {
  label: string;
  value: string;
  onCommit(v: string): void;
  className: string;
  inputMode?: 'decimal' | 'text';
}) {
  const [draft, setDraft] = useState(props.value);
  const editing = useRef(false);
  useEffect(() => {
    if (!editing.current) setDraft(props.value);
  }, [props.value]);
  const commit = (): void => {
    editing.current = false;
    if (draft !== props.value) props.onCommit(draft);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === 'Enter') e.currentTarget.blur();
    if (e.key === 'Escape') {
      setDraft(props.value);
      editing.current = false;
      e.currentTarget.blur();
    }
  };
  return (
    <input
      aria-label={props.label}
      className={props.className}
      value={draft}
      inputMode={props.inputMode}
      autoComplete="off"
      spellCheck={false}
      onFocus={() => {
        editing.current = true;
      }}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={onKey}
    />
  );
}
