// Canvas colors come from the same tokens as the chrome. A canvas cannot read light-dark()
// tokens directly, so hidden probe elements resolve them and the computed rgb is cached.
import { useEffect, useRef } from 'react';
import { subscribePrefs } from '../prefs';

export interface Palette {
  bg: string;
  row: string;
  line: string;
  beat: string;
  bar: string;
  outside: string;
  outKey: string;
  note: string;
  noteEdge: string;
  noteSelected: string;
  playhead: string;
  keyWhite: string;
  keyBlack: string;
  keyLabel: string;
  ink: string;
  inkDim: string;
  panel: string;
  accent: string;
  loop: string;
}

// Class names are spelled out in full so Tailwind can see them.
const PROBES: Record<keyof Palette, string> = {
  bg: 'text-roll-bg',
  row: 'text-roll-row',
  line: 'text-roll-line',
  beat: 'text-roll-beat',
  bar: 'text-roll-bar',
  outside: 'text-roll-outside',
  outKey: 'text-roll-outkey',
  note: 'text-note',
  noteEdge: 'text-note-edge',
  noteSelected: 'text-note-selected',
  playhead: 'text-playhead',
  keyWhite: 'text-key-white',
  keyBlack: 'text-key-black',
  keyLabel: 'text-key-label',
  ink: 'text-ink',
  inkDim: 'text-ink-dim',
  panel: 'text-panel-2',
  accent: 'text-accent',
  loop: 'text-loop',
};

const FALLBACK = '#808080';

export function readPalette(root: HTMLElement): Palette {
  const out = {} as Palette;
  for (const key of Object.keys(PROBES) as (keyof Palette)[]) {
    const el = root.querySelector<HTMLElement>(`[data-token="${key}"]`);
    out[key] = el ? getComputedStyle(el).color : FALLBACK;
  }
  return out;
}

/** Renders the probes and reports a fresh palette now and whenever the color scheme flips. */
export function PaletteProbe({ onPalette }: { onPalette: (p: Palette) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const emit = (): void => onPalette(readPalette(el));
    emit();
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', emit);
    // The theme setting flips color-scheme without a media query change.
    const off = subscribePrefs(emit);
    return () => {
      mq.removeEventListener('change', emit);
      off();
    };
  }, [onPalette]);
  return (
    <div ref={ref} aria-hidden="true" className="hidden">
      {(Object.keys(PROBES) as (keyof Palette)[]).map((k) => (
        <span key={k} data-token={k} className={PROBES[k]} />
      ))}
    </div>
  );
}
