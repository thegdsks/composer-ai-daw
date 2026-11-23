// Note list: the keyboard and screen reader way to see and edit the notes of the open clip.
// Rows are windowed, so a clip with thousands of notes stays light. Edits are the same bulk
// commands the canvas sends.
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { config } from '../config';
import type { Id, Note, Region } from '../model';
import { beatsValue, clampGroupDelta, clampGroupResize, loudnessText, noteSpoken, parseBeats, parsePitch } from '../roll/selection';
import { moveNotesBy, resizeNotesBy, setNotesVelocity } from '../state/bulkNotes';
import { sortedNotes } from '../state/store';
import { meterSegments, pitchName, ticksPerBar } from '../time';
import { field } from './classes';
import { useSession, useSnapshot } from './hooks';

const cell = 'flex items-center px-2';
const input = `${field} h-7 w-full min-w-0 tabular-nums`;

type EditKind = 'pitch' | 'start' | 'length' | 'loudness';

/** Notes of the clip in view, one row each, with the visible slice only in the DOM. */
export function NoteList({ region, trackName }: { region: Region; trackName: string }) {
  const { store } = useSession();
  const snap = useSnapshot();
  const notes = sortedNotes(region);
  const scroller = useRef<HTMLDivElement | null>(null);
  const [view, setView] = useState({ top: 0, height: 0 });
  const [active, setActive] = useState(0);
  const anchor = useRef(0);
  const rowH = config.noteListRowHeight;
  const sig = meterSegments(snap.project.tempo_map)[0]?.sig ?? { numerator: 4, denominator: 4 };
  const bar = ticksPerBar(sig);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const read = (): void => setView({ top: el.scrollTop, height: el.clientHeight });
    read();
    const ro = new ResizeObserver(read);
    // Border box: the spacer padding squeezes the content box to nothing, so it would hide resizes.
    ro.observe(el, { box: 'border-box' });
    el.addEventListener('scroll', read, { passive: true });
    return () => {
      ro.disconnect();
      el.removeEventListener('scroll', read);
    };
  }, []);

  const first = Math.max(0, Math.floor(view.top / rowH) - config.noteListOverscan);
  const last = Math.min(notes.length, Math.ceil((view.top + view.height) / rowH) + config.noteListOverscan);
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.style.setProperty('--list-lead', `${first * rowH}px`);
    el.style.setProperty('--list-trail', `${Math.max(0, notes.length - last) * rowH}px`);
  }, [first, last, notes.length, rowH]);

  const focusRow = useCallback(
    (i: number): void => {
      const n = Math.min(Math.max(0, i), notes.length - 1);
      setActive(n);
      const el = scroller.current;
      if (el) {
        if (n * rowH < el.scrollTop) el.scrollTop = n * rowH;
        else if ((n + 1) * rowH > el.scrollTop + el.clientHeight) el.scrollTop = (n + 1) * rowH - el.clientHeight;
      }
      requestAnimationFrame(() => document.getElementById(`note-row-${n}`)?.focus());
    },
    [notes.length, rowH],
  );

  const select = (i: number, e: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }): void => {
    const n = notes[i];
    if (!n) return;
    if (e.shiftKey) {
      const [a, b] = [Math.min(anchor.current, i), Math.max(anchor.current, i)];
      store.selectNotes(notes.slice(a, b + 1).map((x) => x.id));
    } else if (e.ctrlKey || e.metaKey) {
      const next = new Set(snap.selectedNotes);
      if (next.has(n.id)) next.delete(n.id);
      else next.add(n.id);
      store.selectNotes(next);
      anchor.current = i;
    } else {
      store.selectNotes([n.id]);
      anchor.current = i;
    }
    setActive(i);
  };

  const edit = async (note: Note, kind: EditKind, text: string): Promise<void> => {
    const region0 = region;
    const id: Id[] = [note.id];
    if (kind === 'pitch') {
      const p = parsePitch(text);
      if (p === null) return store.say(`"${text}" is not a note name. Try C4 or F#3`);
      const d = clampGroupDelta(region0, [note], 0, p - note.pitch);
      const moved = { ...note, pitch: note.pitch + d.dPitch };
      await store.group(`Moved to ${noteSpoken(moved, region0.start, bar)}`, moveNotesBy(region0.id, id, 0, d.dPitch));
    } else if (kind === 'start') {
      const t = parseBeats(text);
      if (t === null) return store.say(`"${text}" is not a number of beats`);
      const d = clampGroupDelta(region0, [note], t - note.start, 0);
      const moved = { ...note, start: note.start + d.dTick };
      await store.group(`Moved to ${noteSpoken(moved, region0.start, bar)}`, moveNotesBy(region0.id, id, d.dTick, 0));
    } else if (kind === 'length') {
      const t = parseBeats(text);
      if (t === null || t < 1) return store.say(`"${text}" is not a length in beats`);
      const delta = clampGroupResize(region0, [note], t - note.duration, 1);
      await store.group('Resize note', resizeNotesBy(region0.id, id, delta));
    } else {
      const v = Number(text);
      if (!Number.isInteger(v) || v < 1 || v > 127) return store.say('Loudness is a whole number from 1 to 127');
      await store.group('Set loudness', setNotesVelocity(region0.id, id, v));
    }
  };

  const onRowKey = (e: KeyboardEvent<HTMLDivElement>, i: number): void => {
    if (e.target !== e.currentTarget) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (e.shiftKey) select(Math.min(notes.length - 1, i + 1), { shiftKey: true, ctrlKey: false, metaKey: false });
      else if (!e.ctrlKey) select(Math.min(notes.length - 1, i + 1), e);
      focusRow(i + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (e.shiftKey) select(Math.max(0, i - 1), { shiftKey: true, ctrlKey: false, metaKey: false });
      else select(Math.max(0, i - 1), e);
      focusRow(i - 1);
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      focusRow(e.key === 'Home' ? 0 : notes.length - 1);
    } else if (e.key === ' ') {
      e.preventDefault();
      select(i, { shiftKey: false, ctrlKey: true, metaKey: false });
    } else if (e.key === 'Enter') {
      e.preventDefault();
      (e.currentTarget.querySelector('input') as HTMLInputElement | null)?.focus();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      store.selectNotes(notes.map((n) => n.id));
    }
  };

  if (notes.length === 0) {
    return (
      <p className="m-0 p-4 text-sm text-ink-dim" data-testid="note-list-empty">
        This clip has no notes yet. Switch to the piano roll and draw one, or press Enter there.
      </p>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="note-list">
      <div
        role="grid"
        aria-label={`Notes of ${trackName}, ${notes.length} notes`}
        aria-rowcount={notes.length + 1}
        aria-multiselectable="true"
        className="flex min-h-0 flex-1 flex-col text-sm"
      >
        <div role="row" aria-rowindex={1} className="grid shrink-0 grid-cols-[6rem_6rem_1fr_1fr_1fr] border-b border-line bg-panel-2 text-xs font-medium text-ink-dim">
          <div role="columnheader" className={`${cell} h-8`}>Pitch</div>
          <div role="columnheader" className={`${cell} h-8`}>Bar and beat</div>
          <div role="columnheader" className={`${cell} h-8`}>Start (beats)</div>
          <div role="columnheader" className={`${cell} h-8`}>Length (beats)</div>
          <div role="columnheader" className={`${cell} h-8`}>Loudness</div>
        </div>
        <div className="relative min-h-0 flex-1">
        <div ref={scroller} className="absolute inset-0 overflow-y-auto" data-testid="note-list-scroll" data-window={`${first},${last},${view.top},${view.height}`}>
          <div className="h-(--list-lead,0px)" aria-hidden="true" />
          {notes.slice(first, last).map((n, k) => {
            const i = first + k;
            const selected = snap.selectedNotes.has(n.id);
            const tab = i === active ? 0 : -1;
            const at = region.start + n.start;
            return (
              <div
                key={n.id}
                id={`note-row-${i}`}
                role="row"
                aria-rowindex={i + 2}
                aria-selected={selected}
                tabIndex={tab}
                data-testid="note-row"
                className={`grid h-9 grid-cols-[6rem_6rem_1fr_1fr_1fr] items-center border-b border-line outline-offset-[-2px] focus-visible:outline-2 focus-visible:outline-accent ${
                  selected ? 'bg-accent/15' : 'hover:bg-panel-2'
                }`}
                onKeyDown={(e) => onRowKey(e, i)}
                onClick={(e: MouseEvent) => {
                  if ((e.target as HTMLElement).tagName !== 'INPUT') select(i, e);
                }}
              >
                <Cell label={`Pitch of note ${i + 1}`} value={pitchName(n.pitch)} tab={tab} onCommit={(t) => edit(n, 'pitch', t)} />
                <div role="gridcell" className={`${cell} tabular-nums text-ink-dim`}>
                  {Math.floor(at / bar) + 1}.{Math.floor((at % bar) / config.ticksPerQuarter) + 1}
                </div>
                <Cell label={`Start of note ${i + 1} in beats`} value={beatsValue(n.start)} tab={tab} onCommit={(t) => edit(n, 'start', t)} />
                <Cell label={`Length of note ${i + 1} in beats`} value={beatsValue(n.duration)} tab={tab} onCommit={(t) => edit(n, 'length', t)} />
                <Cell
                  label={`Loudness of note ${i + 1}, ${loudnessText(n.velocity)}`}
                  value={String(n.velocity)}
                  tab={tab}
                  hint={loudnessText(n.velocity)}
                  onCommit={(t) => edit(n, 'loudness', t)}
                />
              </div>
            );
          })}
          <div className="h-(--list-trail,0px)" aria-hidden="true" />
        </div>
        </div>
      </div>
    </div>
  );
}

/** A text box that commits on Enter or when it loses focus, and puts the value back on Escape. */
function Cell(props: { label: string; value: string; tab: number; hint?: string; onCommit(text: string): Promise<void> | void }) {
  const ref = useRef<HTMLInputElement | null>(null);
  const commit = (): void => {
    const el = ref.current;
    if (!el) return;
    if (el.value.trim() === props.value) return;
    void props.onCommit(el.value);
    el.value = props.value;
  };
  return (
    <div role="gridcell" className={cell}>
      <input
        key={props.value}
        ref={ref}
        type="text"
        inputMode="text"
        defaultValue={props.value}
        aria-label={props.label}
        title={props.hint}
        tabIndex={props.tab}
        className={input}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            commit();
            (e.currentTarget.closest('[role="row"]') as HTMLElement | null)?.focus();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            e.currentTarget.value = props.value;
            (e.currentTarget.closest('[role="row"]') as HTMLElement | null)?.focus();
          }
        }}
      />
    </div>
  );
}
