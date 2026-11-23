import { Dialog } from './Dialog';

export type EnvIssueKind = 'no-worklet' | 'no-shared-memory' | 'storage-full' | 'midi-denied' | 'midi-missing';

export interface EnvIssue {
  kind: EnvIssueKind;
  title: string;
  body: string;
  /** One concrete next step. */
  action: string;
}

const ISSUES: Record<EnvIssueKind, EnvIssue> = {
  'no-worklet': {
    kind: 'no-worklet',
    title: 'This browser cannot play sound here',
    body: 'Composer AI DAW needs a feature called AudioWorklet to make sound, and this browser does not offer it. You can still edit and save, but you will not hear anything.',
    action: 'Open Composer AI DAW in a current version of Chrome, Edge, Firefox or Safari.',
  },
  'no-shared-memory': {
    kind: 'no-shared-memory',
    title: 'Playback may stutter on this page',
    body: 'This page was not opened in the isolated mode that gives audio its steadiest timing, so long songs may glitch.',
    action: 'Reload from the address Composer AI DAW gave you, not from a saved copy or an embedded frame.',
  },
  'storage-full': {
    kind: 'storage-full',
    title: 'Your browser has no room to save',
    body: 'Composer AI DAW could not keep a copy of your work in this browser because its storage is full or blocked. Your changes are safe until you close this tab.',
    action: 'Use Save to file now to keep a copy, then free some space in your browser settings.',
  },
  'midi-denied': {
    kind: 'midi-denied',
    title: 'MIDI keyboard access was blocked',
    body: 'The browser did not let Composer AI DAW talk to your MIDI keyboard. This usually means the permission was declined.',
    action: 'Click the lock icon in the address bar, allow MIDI, then press MIDI keyboard again.',
  },
  'midi-missing': {
    kind: 'midi-missing',
    title: 'No MIDI keyboard is available',
    body: 'This browser cannot use MIDI devices. You can still play notes with the computer keyboard (A to ;).',
    action: 'Try Chrome or Edge if you want to use a MIDI keyboard.',
  },
};

export function envIssue(kind: EnvIssueKind): EnvIssue {
  return ISSUES[kind];
}

/** Problems that can be known before the first sound: no AudioWorklet, no isolated memory. */
export function detectEnvIssue(): EnvIssue | null {
  if (typeof AudioWorkletNode === 'undefined') return ISSUES['no-worklet'];
  if (typeof SharedArrayBuffer === 'undefined' || !globalThis.crossOriginIsolated) return ISSUES['no-shared-memory'];
  return null;
}

/** Turns a failed requestMIDIAccess into the matching plain dialog. */
export function midiIssue(e: unknown): EnvIssue {
  const name = e instanceof DOMException ? e.name : '';
  return name === 'NotAllowedError' || name === 'SecurityError' ? ISSUES['midi-denied'] : ISSUES['midi-missing'];
}

/** A plain dialog for a known environment problem, replacing a raw alert. */
export function EnvDialog(props: { issue: EnvIssue | null; onClose(): void }) {
  const { issue } = props;
  return (
    <Dialog
      open={issue !== null}
      onClose={props.onClose}
      title={issue?.title ?? ''}
      testId="env-dialog"
      kind={issue?.kind}
      primary={{ label: 'OK' }}
      secondary={false}
    >
      <p className="m-0 text-sm">{issue?.body}</p>
      <p className="m-0 text-sm font-medium">{issue?.action}</p>
    </Dialog>
  );
}
