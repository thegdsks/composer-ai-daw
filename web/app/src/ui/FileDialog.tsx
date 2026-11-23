import { useState } from 'react';
import { config } from '../config';
import { safeFileName } from '../persistence';
import { field } from './classes';
import { Dialog } from './Dialog';
import { useSession, useSnapshot } from './hooks';

export type FileDialogMode = 'open' | 'save';

/** Asks for a file name inside the daemon folder, then opens or saves through the actions. */
export function FileDialog(props: { mode: FileDialogMode | null; onClose(): void; /** Called once the file was saved or opened. */ onDone?(mode: FileDialogMode): void }) {
  const { actions } = useSession();
  const snap = useSnapshot();
  const [name, setName] = useState('');
  const [overwrite, setOverwrite] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { mode } = props;
  const projectName = snap.project.name;

  // Fresh fields every time the dialog opens (state adjusted during render, not in an effect).
  const [seen, setSeen] = useState<FileDialogMode | null>(null);
  if (mode !== seen) {
    setSeen(mode);
    if (mode) {
      setName(safeFileName(projectName, config.projectExtension));
      setOverwrite(false);
      setError(null);
    }
  }

  const submit = async (): Promise<void> => {
    if (!mode || !name.trim()) return;
    setBusy(true);
    setError(null);
    const problem = mode === 'save' ? await actions.saveToDaemon(name, overwrite) : await actions.openFromDaemon(name);
    setBusy(false);
    if (problem) setError(problem);
    else {
      props.onDone?.(mode);
      props.onClose();
    }
  };

  const title = mode === 'open' ? 'Open a project from the daemon folder' : 'Save the project in the daemon folder';
  return (
    <Dialog
      open={mode !== null}
      onClose={props.onClose}
      title={title}
      onSubmit={() => void submit()}
      primary={{ label: mode === 'open' ? 'Open' : 'Save', disabled: busy || !name.trim() }}
    >
      <label className="flex flex-col gap-1 text-sm text-ink-dim">
        File name
        <input
          data-autofocus=""
          className={`${field} text-ink`}
          value={name}
          spellCheck={false}
          autoComplete="off"
          aria-describedby="file-dialog-hint file-dialog-error"
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <p id="file-dialog-hint" className="m-0 text-xs text-ink-dim">
        A plain name inside the folder the daemon was started in. Files end in .{config.projectExtension}.
      </p>
      {mode === 'save' && (
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={overwrite} onChange={(e) => setOverwrite(e.target.checked)} />
          Replace existing file
        </label>
      )}
      {mode === 'open' && <p className="m-0 text-xs text-ink-dim">Opening replaces the current project and clears undo history.</p>}
      <p id="file-dialog-error" role="alert" className="m-0 min-h-5 text-sm text-danger">
        {error ?? ''}
      </p>
    </Dialog>
  );
}
