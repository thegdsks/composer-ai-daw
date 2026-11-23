// Ctrl K command palette. Entries come from commands.describe (plain labels and search words)
// and commands.enabled (reasons), so the list is never a second copy of the command set.
// Running an entry uses the same Actions and Store calls as the rest of the UI.
import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { newId } from '../ids';
import type { CommandInfo } from '../model';
import { SearchIcon } from '../icons';
import {
  buildCommand,
  bumpUsage,
  buildItems,
  loadUsage,
  matchItems,
  planForm,
  type FormPlan,
  type PaletteContext,
  type PaletteItem,
  type Usage,
} from '../state/palette';
import { plainError } from '../state/errors';
import type { AssistCard as Card } from '../state/assist';
import { AssistCard } from './AssistCard';
import { useAssistCards } from './useAssist';
import { btn, btnPrimary, field } from './classes';
import { useSession, useSnapshot } from './hooks';

/** What the palette can ask the app shell to do. */
export interface PaletteApp {
  play(): void;
  undo(): void;
  redo(): void;
  history(): void;
  help(): void;
  start(): void;
  /** Selection edits from the clipboard actions; `do` names which. */
  edit(what: 'copy' | 'cut' | 'paste' | 'duplicate' | 'repeat'): void;
  transport(what: 'loop' | 'metronome'): void;
  /** Quantize the selected notes (all notes when none are selected). */
  quantize(): void;
}

/** The dialog shell; the body mounts on open, so every open starts with fresh state. */
export function CommandPalette(props: { open: boolean; onClose(): void; app: PaletteApp }) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (props.open && !d.open) d.showModal();
    if (!props.open && d.open) d.close();
  }, [props.open]);
  return (
    <dialog
      ref={ref}
      aria-label="Command palette"
      data-testid="palette-dialog"
      className="m-auto mt-[12vh] w-[34rem] max-w-[94vw] overscroll-contain rounded-lg border border-line bg-panel p-0 text-ink backdrop:bg-black/50"
      onClose={props.onClose}
    >
      {props.open && <PaletteBody onClose={props.onClose} app={props.app} />}
    </dialog>
  );
}

function PaletteBody(props: { onClose(): void; app: PaletteApp }) {
  const { store, actions } = useSession();
  const snap = useSnapshot();
  const input = useRef<HTMLInputElement>(null);
  const [infos, setInfos] = useState<CommandInfo[]>([]);
  const [enabled, setEnabled] = useState<{ name: string; enabled: boolean; reason: string | null }[]>([]);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const [usage, setUsage] = useState<Usage>(loadUsage);
  const [form, setForm] = useState<{ item: PaletteItem; plan: FormPlan } | null>(null);
  const [values, setValues] = useState<Record<string, string | boolean>>({});
  const [message, setMessage] = useState<string | null>(null);
  const [openCard, setOpenCard] = useState<Card | null>(null);
  const assist = useAssistCards();

  const ctx: PaletteContext = useMemo(
    () => ({
      project: snap.project,
      track: snap.selectedTrack,
      region: snap.selectedRegion,
      notes: [...snap.selectedNotes],
    }),
    [snap.project, snap.selectedTrack, snap.selectedRegion, snap.selectedNotes],
  );

  // Fetch the command list and availability once per open, for the selection at that moment.
  useEffect(() => {
    let live = true;
    const snapNow = store.getSnapshot();
    const context = {
      ...(snapNow.selectedTrack ? { track: snapNow.selectedTrack } : {}),
      ...(snapNow.selectedRegion ? { region: snapNow.selectedRegion } : {}),
    };
    void Promise.all([store.backend.describe(), store.backend.enabled(context)]).then(([d, e]) => {
      if (!live) return;
      if (d.ok) setInfos(d.value);
      else setMessage(plainError(d.error));
      setEnabled(e.ok ? e.value : []);
    });
    return () => {
      live = false;
    };
  }, [store]);

  const items = useMemo(() => buildItems(infos, enabled, ctx), [infos, enabled, ctx]);
  const assistItems = useMemo(() => assist.cards.map(cardItem), [assist.cards]);
  const assistRows = useMemo(() => matchItems(assistItems, query, usage), [assistItems, query, usage]);
  const results = useMemo(() => [...assistRows, ...matchItems(items, query, usage)], [assistRows, items, query, usage]);
  const current = results[Math.min(active, results.length - 1)];

  useEffect(() => {
    if (!form && !openCard) input.current?.focus();
  }, [form, infos, openCard]);

  useEffect(() => {
    document.getElementById(`palette-opt-${Math.min(active, results.length - 1)}`)?.scrollIntoView({ block: 'nearest' });
  }, [active, results.length]);

  const finish = (item: PaletteItem): void => {
    setUsage((u) => bumpUsage(u, item.id));
    props.onClose();
  };

  const runApp = (name: string): void => {
    const a = props.app;
    const table: Record<string, () => void> = {
      play: a.play,
      undo: a.undo,
      redo: a.redo,
      history: a.history,
      help: a.help,
      start: a.start,
      copy: () => a.edit('copy'),
      cut: () => a.edit('cut'),
      paste: () => a.edit('paste'),
      duplicate: () => a.edit('duplicate'),
      repeat: () => a.edit('repeat'),
      quantize: a.quantize,
      loop: () => a.transport('loop'),
      metronome: () => a.transport('metronome'),
    };
    table[name]?.();
  };

  const run = async (item: PaletteItem): Promise<void> => {
    if (!item.enabled) {
      setMessage(item.reason ?? 'Not available right now.');
      store.say(`${item.label} is not available. ${item.reason ?? ''}`);
      return;
    }
    setMessage(null);
    if (item.id.startsWith('assist:')) {
      setOpenCard(assist.cards.find((c) => `assist:${c.key}` === item.id) ?? null);
      return;
    }
    if (!item.info) {
      finish(item);
      runApp(item.id.slice(4));
      return;
    }
    // The same Actions the buttons use, so a palette run and a click leave the same history step.
    if (item.id === 'add_track') {
      finish(item);
      await actions.addTrack();
      return;
    }
    if (item.id === 'delete_region' && snap.selectedRegions.size > 0) {
      finish(item);
      await actions.deleteClips([...snap.selectedRegions]);
      return;
    }
    const plan = planForm(item.info, ctx, newId);
    if (plan.ask.length === 0) {
      const built = buildCommand(item.info, plan, {});
      if (!built.ok) return setMessage(built.message);
      const r = await store.apply(built.command);
      if (!r.ok) return setMessage(plainError(r.error));
      return finish(item);
    }
    setValues(Object.fromEntries(plan.ask.map((f) => [f.name, f.initial])));
    setForm({ item, plan });
  };

  const submit = async (): Promise<void> => {
    if (!form?.item.info) return;
    const built = buildCommand(form.item.info, form.plan, values);
    if (!built.ok) return setMessage(built.message);
    const r = await store.apply(built.command);
    if (!r.ok) return setMessage(plainError(r.error));
    finish(form.item);
  };

  const onInputKey = (e: React.KeyboardEvent): void => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActive((a) => Math.min(results.length - 1, a + 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === 'Enter' && current) {
      e.preventDefault();
      void run(current);
    }
  };

  return (
    <>
      {openCard ? (
        <AssistCard
          card={openCard}
          onBack={() => {
            setOpenCard(null);
            assist.reload();
          }}
          onDone={props.onClose}
        />
      ) : form ? (
        <form
          className="flex flex-col gap-3 p-4"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <h2 className="text-base font-semibold">{form.item.label}</h2>
          {form.plan.ask.map((f, i) => (
            <label key={f.name} className="flex flex-col gap-1 text-sm">
              {f.label}
              {f.kind === 'select' ? (
                <select
                  className={field}
                  autoFocus={i === 0}
                  value={String(values[f.name] ?? f.initial)}
                  onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.value }))}
                >
                  {f.options?.map((o) => (
                    <option key={o.value} value={o.value}>
                      {o.label}
                    </option>
                  ))}
                </select>
              ) : f.kind === 'checkbox' ? (
                <input
                  type="checkbox"
                  className="size-5 accent-accent"
                  autoFocus={i === 0}
                  checked={values[f.name] === true}
                  onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.checked }))}
                />
              ) : (
                <input
                  type={f.kind === 'number' ? 'number' : 'text'}
                  className={field}
                  autoFocus={i === 0}
                  min={f.min}
                  max={f.max}
                  step={f.step}
                  value={String(values[f.name] ?? f.initial)}
                  onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.value }))}
                />
              )}
            </label>
          ))}
          <p role="status" className="min-h-5 text-sm text-ink-dim">
            {message}
          </p>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              className={btn}
              onClick={() => {
                setForm(null);
                setMessage(null);
              }}
            >
              Back
            </button>
            <button type="submit" className={btnPrimary} data-primary="true">
              Do it
            </button>
          </div>
        </form>
      ) : (
        <div className="flex flex-col">
          <div className="flex items-center gap-2 border-b border-line px-3">
            <SearchIcon size={18} aria-hidden="true" />
            <input
              ref={input}
              type="text"
              role="combobox"
              aria-label="Type what you want to do"
              aria-expanded="true"
              aria-controls="palette-list"
              aria-activedescendant={current ? `palette-opt-${Math.min(active, results.length - 1)}` : undefined}
              aria-autocomplete="list"
              autoComplete="off"
              spellCheck={false}
              placeholder="Type what you want to do, such as add track"
              className="h-11 min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-ink-dim"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setActive(0);
                setMessage(null);
              }}
              onKeyDown={onInputKey}
            />
          </div>
          <ul id="palette-list" role="listbox" aria-label="Commands" className="m-0 max-h-[50vh] list-none overflow-y-auto overscroll-contain p-1">
            {results.map((item, i) => (
              <Fragment key={item.id}>
                {(i === 0 || i === assistRows.length) && (
                  <li role="presentation" className="px-3 pb-0.5 pt-1.5 text-xs font-medium text-ink-dim">
                    {i < assistRows.length ? 'Suggestions' : 'Commands'}
                  </li>
                )}
                <li
                  id={`palette-opt-${i}`}
                  role="option"
                  aria-selected={i === active}
                  aria-disabled={!item.enabled}
                  data-testid="palette-item"
                  data-command={item.id}
                  className={`flex cursor-pointer flex-col gap-0.5 rounded-md px-3 py-1.5 text-sm ${i === active ? 'bg-panel-2' : ''} ${item.enabled ? '' : 'text-ink-dim'}`}
                  onMouseMove={() => setActive(i)}
                  onClick={() => void run(item)}
                >
                  <span className="flex items-center justify-between gap-2">
                    <span className="truncate">{item.label}</span>
                    {item.keys && <span className="shrink-0 font-mono text-xs text-ink-dim">{item.keys}</span>}
                  </span>
                  {!item.enabled && item.reason && <span className="text-xs">{item.reason}</span>}
                </li>
              </Fragment>
            ))}
            {results.length === 0 && <li className="px-3 py-2 text-sm text-ink-dim">Nothing matches. Try a shorter word.</li>}
          </ul>
          <p role="status" className="min-h-5 border-t border-line px-3 py-1 text-xs text-ink-dim">
            {message ?? 'Up and Down to choose, Enter to run, Esc to close.'}
          </p>
        </div>
      )}
    </>
  );
}

/** A suggestion card as a palette row, so it is found by typing and reached with the arrow keys. */
function cardItem(card: Card): PaletteItem {
  return {
    id: `assist:${card.key}`,
    label: card.label,
    verb: 'suggest',
    keywords: ['ask', 'ai', 'suggest', 'suggestion', card.tag.toLowerCase(), ...card.rationale.toLowerCase().split(/[^a-z]+/).filter((w) => w.length > 3)],
    enabled: true,
    reason: null,
    info: null,
    keys: card.tag,
  };
}
