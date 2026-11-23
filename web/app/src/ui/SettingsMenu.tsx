import { useSyncExternalStore } from 'react';
import { SettingsIcon } from '../icons';
import { getPrefs, setPrefs, subscribePrefs, type Density, type Theme } from '../prefs';
import { iconBtn } from './classes';

const THEMES: { id: Theme; label: string }[] = [
  { id: 'system', label: 'Match my device' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
];

const DENSITIES: { id: Density; label: string }[] = [
  { id: 'comfortable', label: 'Comfortable' },
  { id: 'compact', label: 'Compact' },
];

function Choice<T extends string>(props: {
  legend: string;
  name: string;
  value: T;
  options: { id: T; label: string }[];
  onChange(v: T): void;
}) {
  return (
    <fieldset className="m-0 flex flex-col gap-1 border-0 p-0">
      <legend className="mb-1 p-0 text-sm font-semibold">{props.legend}</legend>
      {props.options.map((o) => (
        <label key={o.id} className="flex min-h-8 items-center gap-2 text-[13px]">
          <input
            type="radio"
            className="size-4 accent-accent"
            name={props.name}
            value={o.id}
            checked={props.value === o.id}
            onChange={() => props.onChange(o.id)}
          />
          {o.label}
        </label>
      ))}
    </fieldset>
  );
}

/** Theme and density live behind one settings button, never in the main bar. */
export function SettingsMenu() {
  const prefs = useSyncExternalStore(subscribePrefs, getPrefs);
  return (
    <>
      <button type="button" className={iconBtn} popoverTarget="settings-popover" aria-label="Settings" title="Settings" data-testid="settings-button">
        <SettingsIcon size={20} />
      </button>
      <div
        id="settings-popover"
        popover="auto"
        aria-label="Settings"
        role="dialog"
        className="fixed inset-auto right-3 top-14 m-0 w-60 flex-col [&:popover-open]:flex gap-4 rounded-md border border-line bg-panel p-4 text-ink shadow-md"
      >
        <Choice legend="Colors" name="theme" value={prefs.theme} options={THEMES} onChange={(theme) => setPrefs({ theme })} />
        <Choice legend="Spacing" name="density" value={prefs.density} options={DENSITIES} onChange={(density) => setPrefs({ density })} />
        <label className="flex min-h-8 items-center gap-2 text-[13px]">
          <input type="checkbox" className="size-4 accent-accent" checked={prefs.toasts} onChange={(e) => setPrefs({ toasts: e.target.checked })} />
          Show short messages (Undo offers always show)
        </label>
      </div>
    </>
  );
}
