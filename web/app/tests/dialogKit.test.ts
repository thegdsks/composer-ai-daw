import { describe, expect, it, vi } from 'vitest';
import { focusReturn, type FocusTarget } from '../src/ui/focusReturn';
import { projectMenuEntries, type ProjectMenuState } from '../src/ui/projectMenuModel';
import { guardLeave, UnsavedGuard } from '../src/state/unsavedGuard';

function el(connected = true): { isConnected: boolean; focus: () => void; calls: number } {
  const t = { isConnected: connected, calls: 0, focus: () => void (t.calls += 1) };
  return t;
}

describe('dialog focus return', () => {
  it('gives focus back to the opener once', () => {
    const opener = el();
    const host = { activeElement: opener as FocusTarget | null };
    const f = focusReturn(host);
    f.capture();
    host.activeElement = el();
    expect(f.restore()).toBe(true);
    expect(opener.calls).toBe(1);
    expect(f.restore()).toBe(false);
  });

  it('does nothing when the opener left the page', () => {
    const opener = el(false);
    const f = focusReturn({ activeElement: opener });
    f.capture();
    expect(f.restore()).toBe(false);
    expect(opener.calls).toBe(0);
  });
});

describe('unsaved work guard', () => {
  it('is clean until the project changes, and clean again at the saved state', () => {
    const g = new UnsavedGuard();
    expect(g.isDirty('a')).toBe(false);
    g.baseline('a');
    g.baseline('b');
    expect(g.isDirty('a')).toBe(false);
    expect(g.isDirty('b')).toBe(true);
    expect(g.isDirty('a')).toBe(false);
  });

  it('markSaved moves the saved point', () => {
    const g = new UnsavedGuard();
    g.baseline('a');
    g.markSaved('c');
    expect(g.isDirty('c')).toBe(false);
    expect(g.isDirty('a')).toBe(true);
  });

  it('asks on leave only when dirty', () => {
    const g = new UnsavedGuard();
    g.baseline('a');
    const clean = { preventDefault: vi.fn(), returnValue: undefined as unknown };
    expect(guardLeave(g, 'a', clean)).toBe(false);
    expect(clean.preventDefault).not.toHaveBeenCalled();
    const dirty = { preventDefault: vi.fn(), returnValue: undefined as unknown };
    expect(guardLeave(g, 'b', dirty)).toBe(true);
    expect(dirty.preventDefault).toHaveBeenCalledTimes(1);
    expect(dirty.returnValue).toBe('');
  });
});

describe('project menu entries', () => {
  const byId = (s: ProjectMenuState) => Object.fromEntries(projectMenuEntries(s).map((e) => [e.id, e]));

  it('export audio needs a note', () => {
    expect(byId({ trackCount: 1, noteCount: 0, busy: false }).audio?.enabled).toBe(false);
    expect(byId({ trackCount: 1, noteCount: 0, busy: false }).audio?.reason).toBeTruthy();
    expect(byId({ trackCount: 1, noteCount: 3, busy: false }).audio?.enabled).toBe(true);
  });

  it('busy turns file entries off but keeps help', () => {
    const e = byId({ trackCount: 1, noteCount: 3, busy: true });
    expect(e.new?.enabled).toBe(false);
    expect(e.save?.enabled).toBe(false);
    expect(e.help?.enabled).toBe(true);
  });

  it('unbuilt entries say why', () => {
    const e = byId({ trackCount: 1, noteCount: 3, busy: false });
    for (const id of ['template', 'midi'] as const) {
      expect(e[id]?.enabled).toBe(false);
      expect(e[id]?.reason).toBeTruthy();
    }
  });
});
