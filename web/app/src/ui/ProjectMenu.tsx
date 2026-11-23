import { useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { ChevronDownIcon, FileIcon } from '../icons';
import { notesOf } from '../model';
import { btn, compact, field } from './classes';
import { Dialog } from './Dialog';
import { FileDialog, type FileDialogMode } from './FileDialog';
import { useSession, useSnapshot } from './hooks';
import { projectMenuEntries, type ProjectMenuId } from './projectMenuModel';
import { useConfirm, useGuard, useToasts } from './shell';

/** One Project button replaces the loose file buttons. Every entry calls an existing action. */
export function ProjectMenu(props: { onHelp(): void; pluginLoaded: boolean }) {
  const { store, actions, backendKind, audio } = useSession();
  const snap = useSnapshot();
  const guard = useGuard();
  const confirm = useConfirm();
  const toasts = useToasts();
  const panel = useRef<HTMLDivElement>(null);
  const importRef = useRef<HTMLInputElement>(null);
  const pluginRef = useRef<HTMLInputElement>(null);
  const id = useId();
  const [fileMode, setFileMode] = useState<FileDialogMode | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);

  const noteCount = useMemo(
    () => snap.project.tracks.reduce((n, t) => n + Object.values(t.regions).reduce((m, r) => m + notesOf(r).length, 0), 0),
    [snap.project],
  );
  const entries = projectMenuEntries({ trackCount: snap.project.tracks.length, noteCount, busy, pluginLoaded: props.pluginLoaded });

  const saved = (): void => guard.markSaved(store.getSnapshot().stateHash);

  /** Replacing the open project asks first when there is work not yet in a file. */
  const okToReplace = async (what: string): Promise<boolean> => {
    if (!guard.isDirty(store.getSnapshot().stateHash)) return true;
    return confirm({
      title: `${what}?`,
      message: 'You have changes that are not saved to a file. They stay in this browser, but this replaces what is open now.',
      confirmLabel: what,
      danger: true,
    });
  };

  const run = async (entry: ProjectMenuId): Promise<void> => {
    switch (entry) {
      case 'new': {
        if (!(await okToReplace('Start a new project'))) return;
        setBusy(true);
        const ok = await actions.applyTemplate('blank');
        setBusy(false);
        if (!ok) return;
        saved();
        toasts.push({ text: 'New project started', action: { label: 'Undo', run: () => store.undo() } });
        return;
      }
      case 'open':
        if (!(await okToReplace('Open another file'))) return;
        if (backendKind === 'daemon') setFileMode('open');
        else importRef.current?.click();
        return;
      case 'save':
        if (backendKind === 'daemon') setFileMode('save');
        else {
          actions.exportJson();
          saved();
        }
        return;
      case 'audio':
        setBusy(true);
        await actions.exportWav();
        setBusy(false);
        return;
      case 'rename':
        setDraft(store.getSnapshot().project.name);
        setRenaming(true);
        return;
      case 'plugin':
        pluginRef.current?.click();
        return;
      case 'unplugin':
        audio.removePlugin();
        toasts.push({ text: 'Plugin removed' });
        return;
      case 'help':
        props.onHelp();
        return;
      default:
        return;
    }
  };

  const choose = (entry: ProjectMenuId, enabled: boolean): void => {
    if (!enabled) return;
    panel.current?.hidePopover();
    void run(entry);
  };

  const items = (): HTMLElement[] => Array.from(panel.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
  const onKeyDown = (e: KeyboardEvent): void => {
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLElement);
    let next = -1;
    if (e.key === 'ArrowDown') next = (at + 1) % all.length;
    else if (e.key === 'ArrowUp') next = (at <= 0 ? all.length : at) - 1;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = all.length - 1;
    if (next < 0) return;
    e.preventDefault();
    all[next]?.focus();
  };

  return (
    <>
      <button type="button" className={compact(btn)} aria-label="Project" popoverTarget={id} aria-haspopup="menu" data-testid="project-menu-button">
        <FileIcon size={18} />
        <span className="max-[2000px]:sr-only">Project</span>
        <ChevronDownIcon size={16} />
      </button>
      <div
        ref={panel}
        id={id}
        popover="auto"
        role="menu"
        aria-label="Project"
        onKeyDown={onKeyDown}
        onToggle={(e) => {
          if (e.nativeEvent instanceof ToggleEvent && e.nativeEvent.newState === 'open') items()[0]?.focus();
        }}
        className="fixed inset-auto right-3 top-24 m-0 w-64 supports-[top:anchor(bottom)]:right-[anchor(right)] supports-[top:anchor(bottom)]:top-[calc(anchor(bottom)+0.25rem)] flex-col [&:popover-open]:flex gap-0.5 rounded-md border border-line bg-panel p-1 text-ink shadow-md"
      >
        {entries.map((e) => (
          <button
            key={e.id}
            type="button"
            role="menuitem"
            aria-disabled={!e.enabled}
            data-menu-id={e.id}
            className="flex min-h-9 flex-col items-start justify-center rounded px-3 py-1 text-left text-sm hover:bg-panel-2 focus-visible:bg-panel-2 aria-disabled:cursor-not-allowed aria-disabled:text-ink-dim"
            onClick={() => choose(e.id, e.enabled)}
          >
            {e.label}
            {!e.enabled && e.reason && <span className="text-xs">{e.reason}</span>}
          </button>
        ))}
      </div>
      <input
        ref={importRef}
        type="file"
        accept="application/json,.json"
        className="hidden"
        aria-label="Open project file"
        tabIndex={-1}
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = '';
          if (file) void actions.importJson(file).then((ok) => ok && saved());
        }}
      />
      <input
        ref={pluginRef}
        type="file"
        accept=".wclap,application/wasm"
        className="hidden"
        aria-label="Plugin file"
        data-testid="plugin-file"
        tabIndex={-1}
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = '';
          if (!file) return;
          setBusy(true);
          void audio.loadPlugin(file).then((r) => {
            setBusy(false);
            toasts.push({ text: r.ok ? `Plugin ready: ${r.info.name}` : `Plugin not loaded: ${r.message}` });
          });
        }}
      />
      <FileDialog mode={fileMode} onClose={() => setFileMode(null)} onDone={saved} />
      <Dialog
        open={renaming}
        onClose={() => setRenaming(false)}
        title="Rename project"
        onSubmit={() => {
          void actions.rename(draft);
          setRenaming(false);
        }}
        primary={{ label: 'Rename', disabled: !draft.trim() }}
      >
        <label className="flex flex-col gap-1 text-sm text-ink-dim">
          Project name
          <input data-autofocus="" className={`${field} text-ink`} value={draft} autoComplete="off" onChange={(e) => setDraft(e.target.value)} />
        </label>
      </Dialog>
    </>
  );
}
