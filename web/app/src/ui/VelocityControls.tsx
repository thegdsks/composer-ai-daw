// Loudness controls for the selected notes: the keyboard and button way to do what a drag in the
// velocity lane does. Sends the same bulk command, so one press is one undo step.
import { useState } from 'react';
import type { Id, Note } from '../model';
import { loudnessName, loudnessText, loudnessValue, type LoudnessName } from '../roll/selection';
import { setNotesVelocity } from '../state/bulkNotes';
import { btnToggle, compact, btn } from './classes';
import { useSession } from './hooks';

const NAMES: LoudnessName[] = ['Soft', 'Medium', 'Loud'];

export function VelocityControls(props: {
  region: Id;
  selected: readonly Note[];
  ramp: boolean;
  onRamp(on: boolean): void;
}) {
  const { store } = useSession();
  const { selected } = props;
  const none = selected.length === 0;
  const first = selected[0]?.velocity ?? 0;
  const same = selected.every((n) => n.velocity === first);
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? (same ? first : Math.round(selected.reduce((s, n) => s + n.velocity, 0) / Math.max(1, selected.length)));

  const apply = (value: number): void => {
    const ids = selected.filter((n) => n.velocity !== value).map((n) => n.id);
    const label = selected.length === 1 ? 'Set loudness' : `Set loudness of ${selected.length} notes`;
    void store.group(label, setNotesVelocity(props.region, ids, value));
  };
  const commitDraft = (): void => {
    if (draft !== null) apply(draft);
    setDraft(null);
  };

  return (
    <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label="Loudness of the selected notes" data-testid="velocity-controls">
      <span className="text-sm text-ink-dim">Loudness</span>
      {NAMES.map((name) => (
        <button
          key={name}
          type="button"
          className={compact(btn)}
          disabled={none}
          title={`Make the selected notes ${name.toLowerCase()} (${loudnessValue(name)})`}
          aria-pressed={!none && same && loudnessName(first) === name}
          onClick={() => apply(loudnessValue(name))}
        >
          {name}
        </button>
      ))}
      <label className="flex items-center gap-1.5 text-sm text-ink-dim">
        <span className="sr-only">Loudness of the selected notes, 1 to 127</span>
        <input
          type="range"
          min={1}
          max={127}
          value={none ? 0 : shown}
          disabled={none}
          className="w-24 accent-accent"
          aria-valuetext={none ? 'No notes selected' : loudnessText(shown)}
          data-testid="velocity-slider"
          onChange={(e) => setDraft(Number(e.target.value))}
          onPointerUp={commitDraft}
          onKeyUp={commitDraft}
          onBlur={commitDraft}
        />
        <span className="w-24 tabular-nums" aria-hidden="true">
          {none ? 'none selected' : loudnessText(shown)}
        </span>
      </label>
      <button
        type="button"
        className={compact(btnToggle(props.ramp))}
        aria-pressed={props.ramp}
        title="Drag across the loudness lane to fade the selected notes (or Alt drag)"
        onClick={() => props.onRamp(!props.ramp)}
      >
        Ramp
      </button>
    </div>
  );
}
