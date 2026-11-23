/** The few things the focus helper needs from a document, so tests can pass a fake. */
export interface FocusHost {
  readonly activeElement: FocusTarget | null;
}

export interface FocusTarget {
  readonly isConnected: boolean;
  focus(): void;
}

/** Remembers who had focus when a dialog opened and gives it back when it closes. */
export function focusReturn(host: FocusHost): { capture(): void; restore(): boolean } {
  let opener: FocusTarget | null = null;
  return {
    capture() {
      opener = host.activeElement;
    },
    restore() {
      const target = opener;
      opener = null;
      if (!target || !target.isConnected) return false;
      target.focus();
      return true;
    },
  };
}
