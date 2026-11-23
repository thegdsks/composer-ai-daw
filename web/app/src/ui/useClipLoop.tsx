import { useCallback, useState, type ReactNode } from 'react';
import { isMidi, type Id } from '../model';
import { findRegion } from '../state/store';
import { loopChoices, loopText, setLoop } from '../state/timelinePlans';
import { meterSegments, ticksPerBar, ticksPerBeat } from '../time';
import { Dialog } from './Dialog';
import { field } from './classes';
import { useSession } from './hooks';

/** Clip loop length: the clip menu item opens this sheet. One set_region_loop per choice, one undo step. */
export function useClipLoop(): { open(regionId: Id): void; text(regionId: Id): string | null; dialog: ReactNode } {
  const { store } = useSession();
  const [target, setTarget] = useState<Id | null>(null);
  const [value, setValue] = useState('0');

  const sizes = useCallback(
    (regionId: Id) => {
      const p = store.getSnapshot().project;
      const found = findRegion(p, regionId);
      if (!found || !isMidi(found.region)) return null;
      const sig = meterSegments(p.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 };
      return { region: found.region, beat: ticksPerBeat(sig), bar: ticksPerBar(sig) };
    },
    [store],
  );

  const open = useCallback(
    (regionId: Id): void => {
      const s = sizes(regionId);
      if (!s) return;
      setValue(String(s.region.loop_length ?? 0));
      setTarget(regionId);
    },
    [sizes],
  );

  const text = useCallback(
    (regionId: Id): string | null => {
      const s = sizes(regionId);
      return s ? loopText(s.region.loop_length ?? 0, s.beat, s.bar) : null;
    },
    [sizes],
  );

  const s = target ? sizes(target) : null;
  const current = s?.region.loop_length ?? 0;
  const choices = s ? loopChoices(s.beat, s.region.length) : [];
  const options = [{ ticks: 0, label: 'Off' }, ...choices.map((c) => ({ ticks: c.ticks, label: loopText(c.ticks, s?.beat ?? 1, s?.bar ?? 1) }))];
  if (current > 0 && !options.some((o) => o.ticks === current) && s) options.push({ ticks: current, label: loopText(current, s.beat, s.bar) });

  const dialog: ReactNode = (
    <Dialog
      open={target !== null && s !== null}
      onClose={() => setTarget(null)}
      title="Clip loop"
      testId="clip-loop-dialog"
      onSubmit={() => {
        if (target) void store.apply(setLoop(target, Number(value)));
        setTarget(null);
      }}
      primary={{ label: 'Set' }}
    >
      <label className="flex flex-col gap-1 text-sm text-ink-dim">
        Repeat the first
        <select data-autofocus="" data-testid="clip-loop-select" className={`${field} text-ink`} value={value} onChange={(e) => setValue(e.target.value)}>
          {options.map((o) => (
            <option key={o.ticks} value={String(o.ticks)}>
              {o.ticks === 0 ? 'Off, play the clip once' : o.label}
            </option>
          ))}
        </select>
      </label>
    </Dialog>
  );

  return { open, text, dialog };
}
