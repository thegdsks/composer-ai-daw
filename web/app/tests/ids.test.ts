import { describe, expect, it } from 'vitest';
import { isId, newId } from '../src/ids';

describe('newId', () => {
  it('makes 32 lower case hex digits', () => {
    for (let i = 0; i < 50; i++) expect(isId(newId())).toBe(true);
  });

  it('does not repeat', () => {
    const seen = new Set<string>();
    for (let i = 0; i < 2000; i++) seen.add(newId());
    expect(seen.size).toBe(2000);
  });

  it('retries the reserved all zero and all one values', () => {
    const queue = [0, 255, 7];
    const id = newId((b) => {
      b.fill(queue.shift() ?? 1);
    });
    expect(id).toBe('07'.repeat(16));
  });
});
