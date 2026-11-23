import { PIANO_KEYS, SHORTCUTS } from '../input/keymap';
import { Dialog } from './Dialog';

/** Shortcut list built from keymap.ts; opens with Shift and /. */
export function Help(props: { open: boolean; onClose(): void; onStart(): void }) {
  return (
    <Dialog
      open={props.open}
      onClose={props.onClose}
      title="Keys and shortcuts"
      width="lg"
      testId="help-dialog"
      primary={{ label: 'Close' }}
      secondary={{
        label: 'Show the start screen',
        onClick: () => {
          props.onClose();
          props.onStart();
        },
      }}
    >
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
        {SHORTCUTS.map((s) => (
          <div key={s.id} className="col-span-2 grid grid-cols-subgrid">
            <dt className="font-mono text-[13px] text-ink">{s.keys}</dt>
            <dd className="m-0">
              {s.label}
              <span className="block text-xs text-ink-dim">{s.where}</span>
            </dd>
          </div>
        ))}
      </dl>
      <h3 className="text-sm font-semibold">Play with the computer keyboard</h3>
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
        {PIANO_KEYS.map((k) => (
          <div key={k.keys} className="col-span-2 grid grid-cols-subgrid">
            <dt className="font-mono text-[13px]">{k.keys}</dt>
            <dd className="m-0">{k.label}</dd>
          </div>
        ))}
      </dl>
    </Dialog>
  );
}
