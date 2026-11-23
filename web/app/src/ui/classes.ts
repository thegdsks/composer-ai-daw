// Shared Tailwind class strings, spelled out in full so the compiler can see them.
export const btn =
  'touch-manipulation select-none inline-flex h-9 items-center justify-center gap-1.5 rounded-md border border-line bg-panel px-3 text-sm font-medium text-ink ' +
  'hover:bg-panel-2 disabled:cursor-not-allowed disabled:opacity-50 motion-safe:transition-colors';

export const btnPrimary =
  'inline-flex h-9 items-center justify-center gap-1.5 rounded-md border border-accent bg-accent px-3 text-sm font-semibold text-accent-ink ' +
  'hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50 motion-safe:transition-opacity';

export const btnSmall =
  'inline-flex h-6 items-center justify-center gap-1 rounded-md border border-line bg-panel px-2 text-xs font-medium text-ink ' +
  'hover:bg-panel-2 motion-safe:transition-colors';

/** A button that stays pressed: used with aria-pressed, the accent fill says "on". */
export const btnToggle = (pressed: boolean): string =>
  `touch-manipulation select-none inline-flex h-9 items-center justify-center gap-1.5 rounded-md border px-3 text-sm font-medium motion-safe:transition-colors ${
    pressed ? 'border-accent bg-accent text-accent-ink hover:opacity-90' : 'border-line bg-panel text-ink hover:bg-panel-2'
  }`;

export const iconBtn =
  'inline-flex size-9 items-center justify-center rounded-md border border-transparent text-ink hover:bg-panel-2 ' +
  'disabled:cursor-not-allowed disabled:opacity-40 motion-safe:transition-colors';

export const field =
  'h-9 rounded-md border border-line bg-panel px-2 text-sm text-ink placeholder:text-ink-dim';

/** The same control one step smaller (a class cannot be overridden reliably by order). */
export const compact = (cls: string): string => cls.replace('h-9', 'h-8').replace('size-9', 'size-8');
