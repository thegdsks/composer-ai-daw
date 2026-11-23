// One suggestion in plain words: what it is, what it changes, a live preview that always rolls
// back, Accept as one undo step, Reject for proposals, and an Undo that lasts until the project moves on.
import { useEffect, useMemo, useRef, useState } from 'react';
import { config } from '../config';
import { blockedReason, commandLine, staleNote, type AssistCard as Card } from '../state/assist';
import { AssistRun } from '../state/assistRun';
import { proposalWatch } from '../state/proposalWatch';
import { btn, btnPrimary } from './classes';
import { useSession, useSnapshot } from './hooks';

interface Applied {
  stateHash: string;
}

export function AssistCard(props: { card: Card; onBack(): void; onDone(): void }) {
  const { card } = props;
  const { store } = useSession();
  const snap = useSnapshot();
  const run = useMemo(() => new AssistRun(store), [store]);
  const heading = useRef<HTMLHeadingElement>(null);
  const [previewing, setPreviewing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [applied, setApplied] = useState<Applied | null>(null);
  const [gone, setGone] = useState(false);
  const [undone, setUndone] = useState(false);

  useEffect(() => {
    heading.current?.focus();
    // Closing the palette in any way must not leave a preview in the project.
    return () => void run.discard();
  }, [run]);

  const blocked = blockedReason(card);
  const stale = staleNote(card);
  const canUndo = applied !== null && snap.stateHash === applied.stateHash && snap.undoDepth > 0;
  const isProposal = card.origin === 'proposal';

  const togglePreview = async (): Promise<void> => {
    setBusy(true);
    if (previewing) {
      await run.discard();
      setPreviewing(false);
      setMessage('Preview undone. Nothing was kept.');
    } else {
      const r = await run.preview(card);
      setPreviewing(r.ok);
      setMessage(r.ok ? 'Previewing. Nothing is kept until you accept.' : r.message);
    }
    setBusy(false);
  };

  const accept = async (): Promise<void> => {
    setBusy(true);
    const r = await run.accept(card);
    setPreviewing(false);
    if (r.ok) {
      setApplied({ stateHash: r.summary.stateHash });
      setMessage('Done. It is one step, so Undo takes it all back.');
      void proposalWatch(store).refresh();
    } else {
      setMessage(r.message);
    }
    setBusy(false);
  };

  const reject = async (): Promise<void> => {
    setBusy(true);
    const r = await run.reject(card);
    setPreviewing(false);
    if (r.ok) {
      setGone(true);
      setMessage('Rejected. The project was not touched.');
      store.say(`Rejected ${card.label}`);
      void proposalWatch(store).refresh();
    } else {
      setMessage(r.message);
    }
    setBusy(false);
  };

  const undo = async (): Promise<void> => {
    setBusy(true);
    await store.undo();
    setApplied(null);
    setUndone(true);
    setMessage('Undone. The project is back as it was.');
    setBusy(false);
  };

  const shown = card.commands.slice(0, config.assistShowMax);

  return (
    <section aria-labelledby="assist-title" data-testid="assist-card" data-origin={card.origin} className="flex flex-col gap-3 p-4">
      <div className="flex items-center gap-2">
        <span data-testid="assist-tag" className="rounded border border-line px-1.5 text-xs font-medium text-ink-dim">
          {card.tag}
        </span>
        {card.author && <span className="truncate text-xs text-ink-dim">from {card.author}</span>}
      </div>
      <h2 id="assist-title" ref={heading} tabIndex={-1} className="text-base font-semibold outline-none">
        {card.label}
      </h2>
      <p className="text-sm text-ink-dim">{card.rationale}</p>
      <ul data-testid="assist-changes" aria-label="What changes" className="m-0 list-disc pl-5 text-sm">
        {card.changes.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <p data-testid="assist-counts" className="text-xs text-ink-dim">
        {card.commandCount} {card.commandCount === 1 ? 'command' : 'commands'}
        {card.applies && `, ${card.changeCount} ${card.changeCount === 1 ? 'edit' : 'edits'} to the project`}.
      </p>
      {stale && <p data-testid="assist-stale" className="rounded-md border border-line bg-panel-2 px-2 py-1 text-sm">{stale}</p>}
      {blocked && !applied && !undone && !gone && (
        <p data-testid="assist-blocked" className="rounded-md border border-line bg-panel-2 px-2 py-1 text-sm">
          {blocked}
        </p>
      )}
      <details className="text-sm">
        <summary className="cursor-pointer text-ink-dim">Show commands</summary>
        <ol data-testid="assist-commands" className="mt-1 max-h-40 list-decimal overflow-y-auto pl-6 font-mono text-xs">
          {shown.map((c, i) => (
            <li key={i} className="break-all">
              {commandLine(c)}
            </li>
          ))}
          {card.commands.length > shown.length && <li className="list-none">and {card.commands.length - shown.length} more</li>}
        </ol>
      </details>
      <p role="status" data-testid="assist-status" className="min-h-5 text-sm text-ink-dim">
        {message}
      </p>
      <div className="flex flex-wrap justify-end gap-2">
        {applied || undone ? (
          <>
            {canUndo && (
              <button type="button" className={btn} data-testid="assist-undo" disabled={busy} onClick={() => void undo()}>
                Undo
              </button>
            )}
            <button type="button" className={btnPrimary} data-primary="true" onClick={props.onDone}>
              Done
            </button>
          </>
        ) : gone ? (
          <button type="button" className={btnPrimary} data-primary="true" onClick={props.onBack}>
            Back to the list
          </button>
        ) : (
          <>
            <button type="button" className={btn} disabled={busy} onClick={props.onBack}>
              Back
            </button>
            {isProposal && (
              <button type="button" className={btn} data-testid="assist-reject" disabled={busy} onClick={() => void reject()}>
                Reject
              </button>
            )}
            <button type="button" className={btn} data-testid="assist-preview" aria-pressed={previewing} disabled={busy || blocked !== null} onClick={() => void togglePreview()}>
              {previewing ? 'Stop preview' : 'Preview'}
            </button>
            <button type="button" className={btnPrimary} data-primary="true" data-testid="assist-accept" disabled={busy || blocked !== null} onClick={() => void accept()}>
              Accept
            </button>
          </>
        )}
      </div>
    </section>
  );
}
