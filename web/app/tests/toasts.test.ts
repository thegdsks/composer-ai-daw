import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { config } from '../src/config';
import { ToastQueue, toastDuration } from '../src/ui/toastQueue';

describe('toast duration rule', () => {
  it('short text is brief, long text stays longer, an action stays longest', () => {
    const long = 'x'.repeat(config.toastLongChars + 1);
    expect(toastDuration('Saved', false)).toBe(config.toastShortMs);
    expect(toastDuration(long, false)).toBe(config.toastLongMs);
    expect(toastDuration('Track deleted', true)).toBe(Math.max(config.toastShortMs, config.toastMs));
  });
});

describe('toast queue', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('stacks toasts and expires each on its own time', () => {
    const q = new ToastQueue();
    q.push({ text: 'one' });
    vi.advanceTimersByTime(config.toastShortMs - 1);
    q.push({ text: 'two' });
    expect(q.getSnapshot().map((t) => t.text)).toEqual(['one', 'two']);
    vi.advanceTimersByTime(1);
    expect(q.getSnapshot().map((t) => t.text)).toEqual(['two']);
    vi.advanceTimersByTime(config.toastShortMs);
    expect(q.getSnapshot()).toHaveLength(0);
  });

  it('drops the oldest past the limit', () => {
    const q = new ToastQueue();
    for (let i = 0; i < config.toastMax + 2; i++) q.push({ text: `t${i}` });
    const list = q.getSnapshot();
    expect(list).toHaveLength(config.toastMax);
    expect(list[0]?.text).toBe('t2');
  });

  it('runs the action once and removes the toast', async () => {
    const q = new ToastQueue();
    const run = vi.fn();
    const id = q.push({ text: 'Track deleted', action: { label: 'Undo', run } });
    expect(id).not.toBeNull();
    await q.act(id as number);
    expect(run).toHaveBeenCalledTimes(1);
    expect(q.getSnapshot()).toHaveLength(0);
  });

  it('silencing drops plain toasts but keeps ones with an action', () => {
    const q = new ToastQueue(() => false);
    expect(q.push({ text: 'Saved' })).toBeNull();
    expect(q.push({ text: 'Track deleted', action: { label: 'Undo', run: () => undefined } })).not.toBeNull();
    expect(q.getSnapshot()).toHaveLength(1);
  });

  it('notifies subscribers on push and dismiss', () => {
    const q = new ToastQueue();
    const seen = vi.fn();
    q.subscribe(seen);
    const id = q.push({ text: 'a' }) as number;
    q.dismiss(id);
    expect(seen).toHaveBeenCalledTimes(2);
  });
});
