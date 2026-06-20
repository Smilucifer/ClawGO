import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { CommitteeResult } from './invest-committee-store.svelte';

// ── transport mock ────────────────────────────────────────────────
const invokeMock = vi.fn();
let eventHandler: ((e: unknown) => void) | null = null;
const listenMock = vi.fn(async (_name: string, cb: (e: unknown) => void) => {
  eventHandler = cb;
  return () => {};
});

vi.mock('$lib/transport', () => ({
  getTransport: () => ({ invoke: invokeMock, listen: listenMock }),
}));

import { InvestCommitteeStore } from './invest-committee-store.svelte';

function makeResult(symbol: string): CommitteeResult {
  return {
    symbol,
    finalVerdict: 'HOLD',
    finalConfidence: 50,
    macroSignal: 'neutral',
    macroStrength: null,
    reasoning: '',
    rounds: [],
    sanityCheck: {
      gate1Pass: true,
      gate2Pass: true,
      finalVerdict: 'HOLD',
      finalConfidence: 50,
      notes: [],
    },
    sentinelOverride: null,
    converged: true,
    totalLatencyMs: 0,
    totalTokens: 0,
  };
}

const streamCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === 'run_committee_stream');

describe('InvestCommitteeStore queue', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue([]);
    eventHandler = null;
  });

  it('enqueues symbols and starts up to maxConcurrent', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 2;
    await store.addToQueue(['A', 'B', 'C']);

    expect(store.queue.map((q) => q.symbol)).toEqual(['A', 'B', 'C']);
    expect(store.runningCount).toBe(2);
    expect(store.queuedCount).toBe(1);
    expect(streamCalls().length).toBe(2);
  });

  it('drains the next queued symbol when one completes', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B']);
    expect(store.runningCount).toBe(1);

    eventHandler?.({ type: 'symbol_complete', symbol: 'A', result: makeResult('A') });
    await Promise.resolve();

    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('done');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('running');
  });

  it('abortSymbol cancels and frees the slot for the next symbol', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B']);

    await store.abortSymbol('A');
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === 'abort_committee_symbol' && (c[1] as { symbol: string }).symbol === 'A',
      ),
    ).toBe(true);
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('running');
  });

  it('abortAll cancels all running and clears queued', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B', 'C']);

    await store.abortAll();
    expect(invokeMock.mock.calls.some((c) => c[0] === 'abort_committee_all')).toBe(true);
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'C')?.status).toBe('aborted');
  });

  it('retrySymbol re-enqueues a finished symbol at the tail', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 5;
    await store.addToQueue(['A']);
    eventHandler?.({ type: 'symbol_complete', symbol: 'A', result: makeResult('A') });
    await Promise.resolve();
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('done');

    await store.retrySymbol('A');
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('running');
  });

  it('ignores symbols already running (dedup)', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 5;
    await store.addToQueue(['A']);
    const before = streamCalls().length;
    await store.addToQueue(['A']);
    expect(streamCalls().length).toBe(before);
  });

  it('marks symbol aborted on symbol_aborted event and drains next', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B']);

    eventHandler?.({ type: 'symbol_aborted', symbol: 'A' });
    await Promise.resolve();

    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('running');
  });
});

describe('InvestCommitteeStore mode', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue([]);
    eventHandler = null;
  });

  it('effectiveMode defaults: watch→research, hold→holding', () => {
    const store = new InvestCommitteeStore();
    expect(store.effectiveMode('A', 'watch')).toBe('research');
    expect(store.effectiveMode('B', 'hold')).toBe('holding');
  });

  it('effectiveMode honors override over kind default', () => {
    const store = new InvestCommitteeStore();
    store.modeOverrides.set('A', 'holding'); // watch 票被改成实盘
    expect(store.effectiveMode('A', 'watch')).toBe('holding');
  });

  it('setSymbolMode records non-default and persists', async () => {
    const store = new InvestCommitteeStore();
    await store.setSymbolMode('A', 'watch', 'holding'); // 偏离默认
    expect(store.modeOverrides.get('A')).toBe('holding');
    const save = invokeMock.mock.calls.find((c) => c[0] === 'save_committee_mode_overrides');
    expect(save?.[1]).toEqual({ overrides: { A: 'holding' } });
  });

  it('setSymbolMode back to default removes the override', async () => {
    const store = new InvestCommitteeStore();
    store.modeOverrides.set('A', 'holding');
    await store.setSymbolMode('A', 'watch', 'research'); // 回到 watch 默认
    expect(store.modeOverrides.has('A')).toBe(false);
    const save = invokeMock.mock.calls.find((c) => c[0] === 'save_committee_mode_overrides');
    expect(save?.[1]).toEqual({ overrides: {} });
  });
});
