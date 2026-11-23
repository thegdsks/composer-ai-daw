import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore, type ComponentType } from 'react';
import { FilmIcon, HeadphonesIcon, MoonIcon, PlayIcon, UploadIcon, XIcon } from '../icons';
import { getPrefs, setPrefs, subscribePrefs } from '../prefs';
import { currentHash, goTo, parseRoute, subscribeRoute } from '../route';
import type { Session } from '../session';
import { loadAllTemplates, type TemplateId } from '../templates';
import { btn, btnPrimary } from './classes';
import { useSession, useSnapshot } from './hooks';

export interface StartState {
  open: boolean;
  /** Opens the sheet (also the #/start route). */
  show(): void;
  /** Closes it and lands on the project route. */
  hide(): void;
}

/** True when the page asks for no start sheet (automation and deep links): ?start=off. */
function startSuppressed(): boolean {
  return new URLSearchParams(window.location.search).get('start') === 'off';
}

/** The sheet opens on first run and on the #/start route; closing always lands on #/p/<id>. */
export function useStart(session: Session): StartState {
  const snap = useSyncExternalStore(session.store.subscribe, session.store.getSnapshot);
  const hash = useSyncExternalStore(subscribeRoute, currentHash);
  const [auto, setAuto] = useState(
    () => !session.restored && session.backendKind === 'wasm' && getPrefs().start === 'show' && !startSuppressed() && !new URLSearchParams(window.location.search).has('stress'),
  );
  const route = parseRoute(hash);
  const open = route.kind === 'start' || (auto && route.kind === 'none');
  const id = snap.project.id;

  const hide = useCallback(() => {
    setAuto(false);
    goTo({ kind: 'project', id }, true);
  }, [id]);
  const show = useCallback(() => goTo({ kind: 'start' }, false), []);

  // Without the sheet the address still names the project, so a link or a reload lands here.
  useEffect(() => {
    if (!open && route.kind === 'none') goTo({ kind: 'project', id }, true);
  }, [open, route.kind, id]);

  return useMemo(() => ({ open, show, hide }), [open, show, hide]);
}

interface Goal {
  id: TemplateId;
  title: string;
  Icon: ComponentType<{ size?: number; className?: string }>;
}

const GOALS: readonly Goal[] = [
  { id: 'recreate', title: 'Recreate a piece', Icon: HeadphonesIcon },
  { id: 'cinematic', title: 'Cinematic idea', Icon: FilmIcon },
  { id: 'calm-hour', title: 'Calm hour', Icon: MoonIcon },
];

const card =
  'flex w-full items-start gap-3 rounded-md border border-line bg-surface p-3 text-left text-ink hover:bg-panel-2 ' +
  'disabled:cursor-not-allowed disabled:opacity-50 motion-safe:transition-colors';

const linkBtn =
  'inline-flex h-9 items-center rounded-md px-2 text-sm font-medium text-ink-dim underline-offset-2 hover:text-ink hover:underline ' +
  'disabled:cursor-not-allowed disabled:opacity-50';

/** Non modal goal sheet over the workspace: one filled button, three goals, a blank page and the way out. */
export function Start({ state }: { state: StartState }) {
  const session = useSession();
  const { actions, audio, store } = session;
  const snap = useSnapshot();
  const [busy, setBusy] = useState<TemplateId | 'starter' | null>(null);
  const primary = useRef<HTMLButtonElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const templates = useMemo(() => loadAllTemplates(), []);
  const canContinue = session.restored || snap.undoDepth > 0;

  useEffect(() => {
    if (state.open) primary.current?.focus();
  }, [state.open]);

  if (!state.open) return null;

  const finish = (): void => {
    if (getPrefs().start === 'show') setPrefs({ start: 'seen' });
    state.hide();
    requestAnimationFrame(() => document.getElementById('arrangement-canvas')?.focus());
  };

  const playStarter = async (): Promise<void> => {
    setBusy('starter');
    void audio.playFromStart(true);
    store.say('Playing the starter on repeat');
    setBusy(null);
    finish();
  };

  const pick = async (id: TemplateId): Promise<void> => {
    setBusy(id);
    const ok = await actions.applyTemplate(id);
    setBusy(null);
    if (!ok) return;
    if (id !== 'blank') {
      void audio.playFromStart(true);
      store.say(`${templates[id].name} started, playing on repeat. Undo brings back what was here.`);
    }
    finish();
  };

  const never = (): void => {
    setPrefs({ start: 'never' });
    state.hide();
  };

  return (
    <section
      aria-labelledby="start-title"
      data-testid="start-sheet"
      className="absolute inset-x-2 top-2 z-20 mx-auto flex max-h-[calc(100%-1rem)] max-w-2xl flex-col gap-4 overflow-y-auto overscroll-contain rounded-lg border border-line bg-panel p-5 shadow-lg"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          finish();
        }
      }}
    >
      <div className="flex flex-col gap-1">
        <h2 id="start-title" className="m-0 text-[28px] font-semibold leading-tight tracking-tight">
          What do you want to make?
        </h2>
        <p className="m-0 text-sm text-ink-dim">Pick one and it plays at once. You can change everything, and Undo goes back.</p>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <button
          ref={primary}
          type="button"
          data-primary="true"
          className={`${btnPrimary} h-11 px-4 text-base`}
          disabled={busy !== null}
          onClick={() => void playStarter()}
        >
          <PlayIcon size={18} />
          Play the starter
        </button>
        <span className="text-[13px] text-ink-dim">A calm four bar loop in C, ready to change.</span>
      </div>

      <ul className="m-0 flex list-none flex-col gap-2 p-0" aria-label="Starting points">
        {GOALS.map(({ id, title, Icon }) => (
          <li key={id}>
            <button type="button" className={card} data-goal={id} disabled={busy !== null} onClick={() => void pick(id)}>
              <Icon size={22} className="mt-0.5 shrink-0 text-ink" />
              <span className="flex min-w-0 flex-col gap-0.5">
                <span className="text-xl font-semibold leading-snug">{title}</span>
                <span className="text-[13px] text-ink-dim">{templates[id].description}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>

      {canContinue && (
        <div className="flex flex-col gap-1" data-testid="start-recent">
          <h3 className="m-0 text-sm font-semibold">Recent</h3>
          <button type="button" className={`${btn} max-w-full justify-start`} onClick={finish}>
            <span className="truncate">Continue {snap.project.name}</span>
          </button>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-x-1 gap-y-1 border-t border-line pt-3">
        <button type="button" className={linkBtn} disabled={busy !== null} onClick={() => void pick('blank')}>
          Start with a blank project
        </button>
        <button type="button" className={linkBtn} onClick={() => fileInput.current?.click()}>
          <UploadIcon size={16} className="mr-1" />
          Open a project file
        </button>
        <input
          ref={fileInput}
          type="file"
          accept="application/json,.json"
          className="hidden"
          aria-label="Open a project file"
          tabIndex={-1}
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = '';
            if (file) void actions.importJson(file).then(finish);
          }}
        />
        <span className="grow" />
        <button type="button" className={linkBtn} onClick={finish}>
          Not now
        </button>
        <button type="button" className={linkBtn} onClick={never}>
          Never show this
        </button>
      </div>
      <p className="m-0 text-xs text-ink-dim">Bring this back any time from Keys and help (Shift /).</p>
    </section>
  );
}

const STEPS: readonly string[] = [
  'Press Play, or Space, to hear it again.',
  'Click a clip in the tracks to see its notes below.',
  'Play along with the computer keys A S D F G.',
];

/** Three steps over a fresh workspace, shown once and remembered. */
export function FirstRun({ hidden }: { hidden: boolean }) {
  const prefs = useSyncExternalStore(subscribePrefs, getPrefs);
  if (hidden || prefs.firstRunDone || startSuppressed()) return null;
  return (
    <aside
      aria-label="First steps"
      data-testid="first-run"
      className="absolute bottom-2 right-2 z-10 flex w-72 max-w-[calc(100%-1rem)] flex-col gap-2 rounded-lg border border-line bg-panel p-3 shadow-lg"
    >
      <div className="flex items-center justify-between gap-2">
        <h2 className="m-0 text-sm font-semibold">Three steps to start</h2>
        <button
          type="button"
          className="inline-flex size-8 items-center justify-center rounded-md text-ink-dim hover:bg-panel-2 hover:text-ink"
          aria-label="Dismiss the first steps"
          onClick={() => setPrefs({ firstRunDone: true })}
        >
          <XIcon size={16} />
        </button>
      </div>
      <ol className="m-0 flex list-decimal flex-col gap-1 pl-5 text-[13px] text-ink">
        {STEPS.map((s) => (
          <li key={s}>{s}</li>
        ))}
      </ol>
      <button type="button" className={`${btn} self-start`} onClick={() => setPrefs({ firstRunDone: true })}>
        Got it
      </button>
    </aside>
  );
}
