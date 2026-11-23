import { config } from '../config';

export interface ToastAction {
  label: string;
  run(): void | Promise<void>;
}

export interface ToastInput {
  text: string;
  action?: ToastAction;
}

export interface Toast {
  id: number;
  text: string;
  action: ToastAction | null;
  ms: number;
}

/** Plain text goes after 2 s (5 s when long); a toast with a button stays long enough to reach it. */
export function toastDuration(text: string, hasAction: boolean): number {
  const base = text.length > config.toastLongChars ? config.toastLongMs : config.toastShortMs;
  return hasAction ? Math.max(base, config.toastMs) : base;
}

/** The toast list and its timers, kept out of React so the rules can be tested. */
export class ToastQueue {
  private items: Toast[] = [];
  private nextId = 1;
  private readonly timers = new Map<number, ReturnType<typeof setTimeout>>();
  private readonly listeners = new Set<() => void>();

  /** `enabled` is the silence preference; toasts that carry a button are never silenced. */
  constructor(private readonly enabled: () => boolean = () => true) {}

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  getSnapshot = (): readonly Toast[] => this.items;

  private emit(): void {
    for (const l of this.listeners) l();
  }

  /** Returns the toast id, or null when the preference silenced it. */
  push(input: ToastInput): number | null {
    if (!input.action && !this.enabled()) return null;
    const toast: Toast = {
      id: this.nextId++,
      text: input.text,
      action: input.action ?? null,
      ms: toastDuration(input.text, Boolean(input.action)),
    };
    let next = [...this.items, toast];
    while (next.length > Math.max(1, config.toastMax)) {
      const gone = next[0];
      if (!gone) break;
      this.clear(gone.id);
      next = next.slice(1);
    }
    this.items = next;
    this.timers.set(toast.id, setTimeout(() => this.dismiss(toast.id), toast.ms));
    this.emit();
    return toast.id;
  }

  private clear(id: number): void {
    const t = this.timers.get(id);
    if (t) clearTimeout(t);
    this.timers.delete(id);
  }

  dismiss(id: number): void {
    this.clear(id);
    if (!this.items.some((t) => t.id === id)) return;
    this.items = this.items.filter((t) => t.id !== id);
    this.emit();
  }

  /** Runs the toast button, then removes the toast. */
  async act(id: number): Promise<void> {
    const toast = this.items.find((t) => t.id === id);
    this.dismiss(id);
    await toast?.action?.run();
  }

  dispose(): void {
    for (const id of [...this.timers.keys()]) this.clear(id);
    this.items = [];
    this.listeners.clear();
  }
}
