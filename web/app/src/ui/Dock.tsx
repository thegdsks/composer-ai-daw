import { useEffect, useRef, type KeyboardEvent, type ReactNode } from 'react';
import { setNotesTab } from '../prefs';
import { EffectsIcon, KeyboardIcon, NoteIcon, SlidersVerticalIcon, SpeakerIcon } from '../icons';
import { EffectsPanel } from './EffectsPanel';
import { KeysPanel } from './KeysPanel';
import { Mixer } from './mixer/Mixer';
import { MasterStrip } from './MasterStrip';
import { SoundBrowser } from './SoundBrowser';
import { SoundPanel } from './SoundPanel';

export type DockTab = 'notes' | 'sound' | 'keys' | 'mix' | 'effects';

const TABS: { id: DockTab; label: string; icon: ReactNode }[] = [
  { id: 'notes', label: 'Notes', icon: <NoteIcon size={16} /> },
  { id: 'sound', label: 'Sound', icon: <SpeakerIcon size={16} /> },
  { id: 'keys', label: 'Keys', icon: <KeyboardIcon size={16} /> },
  { id: 'mix', label: 'Mix', icon: <SlidersVerticalIcon size={16} /> },
  { id: 'effects', label: 'Effects', icon: <EffectsIcon size={16} /> },
];

const tabBase =
  'inline-flex h-8 items-center gap-1.5 border-b-2 px-3 text-sm font-medium motion-safe:transition-colors';

/** Bottom dock: one tab strip, one panel at a time. Notes stays mounted so its canvas keeps its state. */
export function Dock(props: { tab: DockTab; onTab(t: DockTab): void; notes: ReactNode }) {
  const refs = useRef<Record<DockTab, HTMLButtonElement | null>>({ notes: null, sound: null, keys: null, mix: null, effects: null });
  // The roll gets a taller share of the workspace than the other tabs until the split is dragged.
  useEffect(() => setNotesTab(props.tab === 'notes'), [props.tab]);

  const onKey = (e: KeyboardEvent): void => {
    const i = TABS.findIndex((t) => t.id === props.tab);
    let next: number;
    if (e.key === 'ArrowRight') next = (i + 1) % TABS.length;
    else if (e.key === 'ArrowLeft') next = (i - 1 + TABS.length) % TABS.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = TABS.length - 1;
    else return;
    e.preventDefault();
    const target = TABS[next];
    if (!target) return;
    props.onTab(target.id);
    refs.current[target.id]?.focus();
  };

  return (
    <section aria-label="Dock" className="flex min-h-0 min-w-0 flex-1 flex-col bg-panel">
      <div role="tablist" aria-label="Dock panels" className="flex shrink-0 items-end border-b border-line px-2" onKeyDown={onKey}>
        {TABS.map((t) => {
          const active = t.id === props.tab;
          return (
            <button
              key={t.id}
              ref={(el) => {
                refs.current[t.id] = el;
              }}
              type="button"
              role="tab"
              id={`dock-tab-${t.id}`}
              aria-selected={active}
              aria-controls={`dock-panel-${t.id}`}
              tabIndex={active ? 0 : -1}
              data-testid={`dock-tab-${t.id}`}
              className={`${tabBase} ${active ? 'border-accent text-ink' : 'border-transparent text-ink-dim hover:text-ink'}`}
              onClick={() => props.onTab(t.id)}
            >
              {t.icon}
              {t.label}
            </button>
          );
        })}
      </div>
      <div
        role="tabpanel"
        id="dock-panel-notes"
        aria-labelledby="dock-tab-notes"
        hidden={props.tab !== 'notes'}
        className="flex min-h-0 flex-1 flex-col"
      >
        {props.notes}
      </div>
      {(['sound', 'keys', 'mix', 'effects'] as const).map((id) => (
        <div
          key={id}
          role="tabpanel"
          id={`dock-panel-${id}`}
          aria-labelledby={`dock-tab-${id}`}
          hidden={props.tab !== id}
          className="flex min-h-0 flex-1 flex-col overflow-y-auto"
        >
          {props.tab === id && id === 'keys' && <KeysPanel />}
          {props.tab === id && id === 'effects' && <EffectsPanel />}
          {props.tab === id && id === 'mix' && (
            <>
              <Mixer />
              <MasterStrip />
            </>
          )}
          {props.tab === id && id === 'sound' && (
            <>
              <SoundPanel />
              <SoundBrowser onNotes={() => props.onTab('notes')} />
            </>
          )}
        </div>
      ))}
    </section>
  );
}
