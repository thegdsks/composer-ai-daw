import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { AudioStatus } from '../src/audio/audioEngine';
import { starterCommands } from '../src/fixtures';
import { newId } from '../src/ids';
import { gridChoices } from '../src/config';
import { matchShortcut, SHORTCUTS, spaceMeansPlay } from '../src/input/keymap';
import { Store } from '../src/state/store';
import type { Session } from '../src/session';
import { newBackend } from './helpers';

const store = new Store(newBackend('plain'));
await store.refresh();
await store.group('Starter', starterCommands(newId));

const audioStatus: AudioStatus = {
  state: 'running',
  transport: 'stopped',
  sampleRate: 48000,
  positionFrames: 0,
  lengthFrames: 0,
  counting: false,
  seeks: 0,
  loopWraps: 0,
  clicks: 0,
  clickPeak: 0,
  peak: 0,
  maxPeak: 0,
  trackPeaks: new Float32Array(0),
  clippedFrames: 0,
  blobBytes: 0,
  reloads: 0,
  reports: 0,
  error: null,
  previews: 0,
  previewSound: null,
  plugin: null,
  pluginError: null,
};

const session = {
  store,
  audio: { status: audioStatus, latencyMs: 12, toggle: () => undefined, stop: () => undefined },
  actions: {},
  record: {
    subscribe: () => () => undefined,
    getSnapshot: () => ({ phase: 'idle', tidy: true, gridId: '1/8', pianoKeys: false, lastNotes: 0, lastStarts: [] }),
  },
  catcher: { subscribe: () => () => undefined, getSnapshot: () => ({ count: 0 }), commit: () => Promise.resolve(0) },
  backendKind: 'wasm',
  notice: null,
} as unknown as Session;

vi.mock('../src/ui/hooks', () => ({
  useSession: () => session,
  useSnapshot: () => store.getSnapshot(),
  useAudioStatus: () => audioStatus,
}));

// The settings popover reads browser prefs and is a disclosed panel, so it is left out here.
vi.mock('../src/ui/SettingsMenu', () => ({ SettingsMenu: () => null }));

const { TopBar } = await import('../src/ui/TopBar');
const { StatusBar } = await import('../src/ui/StatusBar');
const { PianoRoll } = await import('../src/ui/PianoRoll');
const { Help } = await import('../src/ui/Help');
const { ConfirmContext, GuardContext, ToastsContext } = await import('../src/ui/shell');
const { ToastQueue } = await import('../src/ui/toastQueue');
const { UnsavedGuard } = await import('../src/state/unsavedGuard');

/** The shell contexts the Project menu needs. */
const withShell = (child: ReactElement): ReactElement =>
  createElement(
    ToastsContext.Provider,
    { value: new ToastQueue() },
    createElement(GuardContext.Provider, { value: new UnsavedGuard() }, createElement(ConfirmContext.Provider, { value: () => Promise.resolve(true) }, child)),
  );

/** Visible words plus the names and tooltips a screen reader or hover shows. */
function chromeText(html: string): string {
  const attrs = [...html.matchAll(/(?:aria-label|title)="([^"]*)"/g)].map((m) => m[1] ?? '');
  const text = html.replace(/<[^>]+>/g, ' ');
  return `${text} ${attrs.join(' ')}`.replace(/\s+/g, ' ');
}

// Jargon belongs in disclosed detail only. The first four come from P4; the rest are the old labels.
const BANNED = [/quantiz/i, /\bregions?\b/i, /\bvelocity\b/i, /MIDI CC/i, /\bSnap\b/, /^Save$/, /\bExport WAV\b/];

describe('plain language chrome', () => {
  const shell = [
    ['top bar', chromeText(renderToStaticMarkup(withShell(createElement(TopBar, { midi: { supported: true, inputs: null, toggle: () => undefined }, onHelp: () => undefined, onPalette: () => undefined, onHistory: () => undefined, historyOpen: false }))))],
    ['status bar', chromeText(renderToStaticMarkup(createElement(StatusBar, { message: null, octave: 0 })))],
    ['piano roll', chromeText(renderToStaticMarkup(createElement(PianoRoll, { onApi: () => undefined, preview: () => undefined })))],
  ] as const;

  it.each(shell)('%s has no banned labels', (_name, text) => {
    for (const re of BANNED) expect(text).not.toMatch(re);
  });

  it('uses the plain names the plan asks for', () => {
    const all = shell.map((s) => s[1]).join(' ');
    expect(all).toContain('Save to file');
    expect(all).toContain('Grid');
    expect(all).toMatch(/Editing: .+, bars 1 to 4/);
    expect(all).toContain('Undo');
  });

  it('puts the grid at eighth notes by default and offers Off', () => {
    const html = renderToStaticMarkup(createElement(PianoRoll, { onApi: () => undefined, preview: () => undefined }));
    expect(html).toMatch(/<option value="1\/8" selected="">Eighth notes<\/option>/);
    expect(html).toContain('<option value="off">Off</option>');
    expect(gridChoices().map((c) => c.label)).toContain('Eighth note triplets');
  });

  it('lists every shortcut in the help overlay', () => {
    const html = renderToStaticMarkup(createElement(Help, { open: false, onClose: () => undefined, onStart: () => undefined }));
    for (const s of SHORTCUTS) expect(html).toContain(s.label);
  });
});

describe('keymap', () => {
  const key = (init: Partial<KeyboardEvent>): KeyboardEvent => ({ ctrlKey: false, metaKey: false, shiftKey: false, code: '', key: '', ...init }) as KeyboardEvent;

  it('maps keys to shortcut ids', () => {
    expect(matchShortcut(key({ code: 'Space', key: ' ' }))).toBe('play');
    expect(matchShortcut(key({ key: 'z', ctrlKey: true }))).toBe('undo');
    expect(matchShortcut(key({ key: 'Z', ctrlKey: true, shiftKey: true }))).toBe('redo');
    expect(matchShortcut(key({ key: '?', code: 'Slash', shiftKey: true }))).toBe('help');
    expect(matchShortcut(key({ key: 'a' }))).toBeNull();
  });
});

/** Just enough of an element for the focus rules; the real DOM check is the e2e script. */
class FakeEl {
  isContentEditable = false;
  constructor(
    readonly tagName: string,
    private readonly inside: string[] = [],
  ) {}
  closest(sel: string): FakeEl | null {
    return this.inside.includes(sel) ? this : null;
  }
}

describe('Space context', () => {
  it('only plays when nothing else owns the key', () => {
    const body = new FakeEl('BODY');
    vi.stubGlobal('HTMLElement', FakeEl);
    vi.stubGlobal('document', { body, documentElement: new FakeEl('HTML') });
    expect(spaceMeansPlay(new FakeEl('BUTTON') as unknown as HTMLElement)).toBe(false);
    expect(spaceMeansPlay(new FakeEl('INPUT') as unknown as HTMLElement)).toBe(false);
    expect(spaceMeansPlay(new FakeEl('BUTTON', ['[data-space-play]']) as unknown as HTMLElement)).toBe(true);
    expect(spaceMeansPlay(new FakeEl('CANVAS') as unknown as HTMLElement)).toBe(true);
    expect(spaceMeansPlay(body as unknown as HTMLElement)).toBe(true);
    vi.unstubAllGlobals();
  });
});
