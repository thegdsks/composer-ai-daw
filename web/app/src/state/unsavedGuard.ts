/**
 * Tracks whether the open project differs from the last file save, open or new project.
 * Compared by state hash, so undoing back to the saved state is clean again.
 */
export class UnsavedGuard {
  private saved: string | null = null;

  /** The first real hash after boot is the starting point. Later calls keep it. */
  baseline(hash: string): void {
    if (this.saved === null && hash) this.saved = hash;
  }

  /** Call after a save to file, an open or a new project. */
  markSaved(hash: string): void {
    this.saved = hash;
  }

  isDirty(hash: string): boolean {
    return this.saved !== null && hash !== this.saved;
  }
}

/** The browser leave prompt: returns true when it asked (the event was cancelled). */
export function guardLeave(guard: UnsavedGuard, hash: string, e: { preventDefault(): void; returnValue: unknown }): boolean {
  if (!guard.isDirty(hash)) return false;
  e.preventDefault();
  // Older browsers only show the prompt when returnValue is set.
  e.returnValue = '';
  return true;
}
