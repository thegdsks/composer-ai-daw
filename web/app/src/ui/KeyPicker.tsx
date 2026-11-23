import type { ScaleName } from '../model';
import { ROOT_NAMES, SCALES, keyAt } from '../roll/keyScale';
import { field } from './classes';
import { useSession, useSnapshot } from './hooks';

/** Project key at tick 0. "Not sure" removes it, so the roll shows no key shading. */
export function KeyPicker() {
  const { store } = useSession();
  const snap = useSnapshot();
  const key = keyAt(snap.project.tempo_map, 0);
  const scale = key?.scale ?? 'major';

  const choose = (root: number, name: ScaleName): void => {
    void store.apply({ command: 'set_key', tick: 0, root, scale: name });
  };

  return (
    <div className="flex items-center gap-1.5" role="group" aria-label="Key">
      <label className="flex items-center gap-1.5 text-sm text-ink-dim">
        <span className="max-[2000px]:sr-only">Key</span>
        <select
          className={field}
          aria-label="Key note"
          data-testid="key-root"
          value={key ? String(key.root) : 'none'}
          onChange={(e) => {
            if (e.target.value === 'none') void store.apply({ command: 'remove_key', tick: 0 });
            else choose(Number(e.target.value), scale);
          }}
        >
          <option value="none">Not sure</option>
          {ROOT_NAMES.map((n, i) => (
            <option key={n} value={String(i)}>
              {n}
            </option>
          ))}
        </select>
      </label>
      <select
        className={`${field} max-[2000px]:w-28`}
        aria-label="Scale"
        data-testid="key-scale"
        disabled={!key}
        value={scale}
        onChange={(e) => key && choose(key.root, e.target.value as ScaleName)}
      >
        {SCALES.map((s) => (
          <option key={s.name} value={s.name}>
            {s.label}
          </option>
        ))}
      </select>
    </div>
  );
}
