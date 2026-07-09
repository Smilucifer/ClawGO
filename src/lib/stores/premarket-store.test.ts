import { describe, it, expect, vi } from 'vitest';
vi.mock('$lib/transport', () => ({ getTransport: () => ({ invoke: vi.fn() }) }));
import { PremarketStore } from './premarket-store.svelte';

describe('PremarketStore timer', () => {
  it('markStart sets generating + startedAt', () => {
    const s = new PremarketStore();
    s.markStart(1000);
    expect(s.generating).toBe(true);
    expect(s.startedAt).toBe(1000);
    expect(s.lastError).toBeNull();
  });

  it('markFinish records elapsed + clears generating + bumps seq', () => {
    const s = new PremarketStore();
    s.markStart(1000);
    const seq0 = s.completionSeq;
    s.markFinish(null, 4000);
    expect(s.generating).toBe(false);
    expect(s.lastElapsedMs).toBe(3000);
    expect(s.completionSeq).toBe(seq0 + 1);
  });

  it('markFinish with error stores message, still bumps seq', () => {
    const s = new PremarketStore();
    s.markStart(0);
    s.markFinish('boom', 100);
    expect(s.lastError).toBe('boom');
    expect(s.generating).toBe(false);
  });
});
