// The one list of app shortcuts. The key handler, the help overlay and the tooltips all read
// from here, so a key is never documented in one place and wired in another.

export type ShortcutId = 'play' | 'undo' | 'redo' | 'delete' | 'help' | 'selectAll' | 'fit' | 'zoom' | 'loop' | 'metronome' | 'seekBack' | 'seekForward' | 'copy' | 'cut' | 'paste' | 'duplicate' | 'repeat' | 'library' | 'record' | 'keys' | 'palette' | 'history' | 'catch' | 'chordPad';

export interface Shortcut {
  id: ShortcutId;
  /** Words a person reads, never raw key codes. */
  keys: string;
  label: string;
  /** Where the shortcut applies, shown as a hint in the help overlay. */
  where: string;
}

export const SHORTCUTS: readonly Shortcut[] = [
  { id: 'play', keys: 'Space', label: 'Play or pause', where: 'When no button or field has focus, or on the transport buttons' },
  { id: 'loop', keys: 'L', label: 'Loop on or off', where: 'In the arrangement or on the transport buttons' },
  { id: 'metronome', keys: 'M', label: 'Metronome on or off', where: 'In the arrangement or on the transport buttons' },
  { id: 'seekBack', keys: 'Comma', label: 'Playhead one bar back', where: 'In the arrangement or on the transport buttons' },
  { id: 'seekForward', keys: 'Period', label: 'Playhead one bar forward', where: 'In the arrangement or on the transport buttons' },
  { id: 'record', keys: 'R', label: 'Record or stop recording', where: 'Anywhere outside a text field. Records into the selected clip after a count in' },
  { id: 'keys', keys: 'K', label: 'Turn the computer keyboard piano on (Shift K turns it off)', where: 'Anywhere outside a text field. It is also on while recording' },
  { id: 'catch', keys: 'Ctrl or Cmd Shift M', label: 'Catch what I just played as a new clip', where: 'Anywhere outside a text field' },
  { id: 'chordPad', keys: '1 to 7', label: 'Play a chord pad', where: 'While the Keys tab is open' },
  { id: 'library', keys: 'Shift B', label: 'Show or hide the sound library', where: 'Anywhere outside a text field' },
  { id: 'undo', keys: 'Ctrl or Cmd Z', label: 'Undo the last step', where: 'Anywhere outside a text field' },
  { id: 'redo', keys: 'Ctrl or Cmd Shift Z', label: 'Redo the step you undid', where: 'Anywhere outside a text field' },
  { id: 'palette', keys: 'Ctrl or Cmd K', label: 'Find a command by name', where: 'Anywhere' },
  { id: 'history', keys: 'Ctrl or Cmd Alt Z', label: 'Show the history of your steps', where: 'Anywhere' },
  { id: 'copy', keys: 'Ctrl or Cmd C', label: 'Copy the selected notes or clips', where: 'In the piano roll or the arrangement' },
  { id: 'cut', keys: 'Ctrl or Cmd X', label: 'Cut the selected notes or clips', where: 'In the piano roll or the arrangement' },
  { id: 'paste', keys: 'Ctrl or Cmd V', label: 'Paste at the playhead bar', where: 'In the piano roll (into the open clip) or the arrangement' },
  { id: 'duplicate', keys: 'Ctrl or Cmd D', label: 'Duplicate right after itself', where: 'In the piano roll or the arrangement' },
  { id: 'repeat', keys: 'Ctrl or Cmd Shift D', label: 'Repeat a number of times', where: 'In the piano roll or the arrangement' },
  { id: 'delete', keys: 'Delete', label: 'Delete the selected notes', where: 'Anywhere outside a text field' },
  { id: 'selectAll', keys: 'Ctrl or Cmd A', label: 'Select every note in the clip', where: 'In the piano roll' },
  { id: 'fit', keys: 'Home', label: 'Frame the whole clip', where: 'In the piano roll' },
  { id: 'zoom', keys: 'Plus and minus', label: 'Zoom in and out', where: 'In the piano roll' },
  { id: 'help', keys: 'Shift /', label: 'Show this list', where: 'Anywhere outside a text field' },
];

/** Keys that are listed here but handled by the live note input. */
export const PIANO_KEYS: readonly { keys: string; label: string }[] = [
  { keys: 'K or the Keys button', label: 'Turn the keys on first. They are also on while you record' },
  { keys: 'A S D F G H J K L ;', label: 'Play the white keys' },
  { keys: 'W E T Y U O P', label: 'Play the black keys' },
  { keys: 'Z and X', label: 'Move the keyboard down or up an octave' },
];

export function shortcutKeys(id: ShortcutId): string {
  return SHORTCUTS.find((s) => s.id === id)?.keys ?? '';
}

/** A tooltip such as "Undo Add note (Ctrl or Cmd Z)". */
export function tip(text: string, id: ShortcutId): string {
  return `${text} (${shortcutKeys(id)})`;
}

/** Text fields keep their own undo and Delete. */
export function isTextField(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ['INPUT', 'TEXTAREA'].includes(target.tagName))
  );
}

/** Fields plus selects and open dialogs: no global key should fire there. */
export function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (isTextField(target) || target.tagName === 'SELECT' || target.closest('dialog') !== null)
  );
}

/**
 * Space means play only where nothing else owns it: the page itself, a canvas, or the transport
 * buttons. On any other focused control Space keeps its normal job (a button activates).
 */
export function spaceMeansPlay(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return true;
  if (isTyping(target)) return false;
  if (target === document.body || target === document.documentElement) return true;
  if (target.tagName === 'CANVAS') return true;
  return target.closest('[data-space-play]') !== null;
}

/** L, M and the bar keys belong to the transport only where letters are not piano keys. */
export function transportKeysApply(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement) || isTyping(target)) return false;
  return target.closest('[data-own-keys], [data-space-play], [data-transport-keys]') !== null;
}

export function matchShortcut(e: KeyboardEvent): ShortcutId | null {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  if (mod && key === 'k') return 'palette';
  if (mod && e.shiftKey && !e.altKey && key === 'm') return 'catch';
  // Alt changes the key value on some layouts, so the physical key decides here.
  if (mod && e.altKey && (e.code === 'KeyZ' || key === 'z')) return 'history';
  if (mod && key === 'z') return e.shiftKey ? 'redo' : 'undo';
  if (mod && key === 'y') return 'redo';
  if (mod) return null;
  if (e.code === 'Space') return 'play';
  if (e.shiftKey && !e.altKey && e.code === 'KeyB') return 'library';
  if (!e.altKey && !e.shiftKey) {
    if (key === 'l') return 'loop';
    if (key === 'm') return 'metronome';
    if (e.code === 'Comma') return 'seekBack';
    if (key === 'r') return 'record';
    if (e.code === 'Period') return 'seekForward';
  }
  if (!e.altKey && key === 'k') return 'keys';
  if (e.key === 'Delete' || e.key === 'Backspace') return 'delete';
  if (e.key === '?' || (e.code === 'Slash' && e.shiftKey)) return 'help';
  return null;
}

/** The pad a plain number key plays (0 for key 1), or null. Only the Keys tab asks for it. */
export function chordPadIndex(e: KeyboardEvent): number | null {
  if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return null;
  const m = /^Digit([1-9])$/.exec(e.code);
  return m?.[1] ? Number(m[1]) - 1 : null;
}
