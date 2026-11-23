import { useEffect } from 'react';
import { CatchButton } from './CatchButton';
import { ChordPads } from './ChordPads';
import { ScreenKeys } from './ScreenKeys';
import { useSession } from './hooks';

/** Dock tab for playing without a MIDI device: chord pads, a clickable keyboard and Catch. */
export function KeysPanel() {
  const { record } = useSession();
  // Opening this tab means "let me play", so the computer keys come on with it.
  useEffect(() => {
    if (!record.keysActive) record.setPianoKeys(true);
  }, [record]);
  return (
    <div className="flex min-h-0 flex-1 flex-wrap gap-4 overflow-y-auto p-3" data-testid="keys-panel">
      <div className="flex min-w-0 flex-1 basis-96 flex-col gap-3">
        <ChordPads />
        <ScreenKeys />
      </div>
      <div className="flex w-44 shrink-0 flex-col items-start gap-2 text-sm text-ink-dim">
        <CatchButton />
        <span>Played something good? Catch keeps the last notes as a clip at the playhead.</span>
      </div>
    </div>
  );
}
