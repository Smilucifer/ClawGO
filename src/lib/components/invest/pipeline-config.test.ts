import { describe, it, expect } from 'vitest';
import { getStepState } from './pipeline-config';
import type { SymbolProgress } from '$lib/stores/invest-committee-store.svelte';

function progress(overrides: Partial<SymbolProgress>): SymbolProgress {
  return {
    activeStep: -1,
    completedSteps: 0,
    completedRounds: [],
    done: false,
    error: null,
    result: null,
    regimeData: null,
    failedSteps: new Set(),
    status: 'running',
    ...overrides,
  };
}

describe('getStepState', () => {
  it('returns aborted for incomplete steps when symbol is aborted', () => {
    const p = progress({ status: 'aborted', completedSteps: 2 });
    // step 4 (quant_r2) not yet completed → aborted
    expect(getStepState(p, 4)).toBe('aborted');
  });

  it('keeps done steps as done even when aborted', () => {
    const p = progress({
      status: 'aborted',
      completedRounds: [
        { role: 'macro', round: 1, label: '', parsed: { rawText: '' }, latencyMs: 0, tokensUsed: 0 },
      ],
    });
    // macro (backendIdx 0) already completed → stays done
    expect(getStepState(p, 0)).toBe('done');
  });

  it('returns pending when no progress', () => {
    expect(getStepState(undefined, 0)).toBe('pending');
  });
});
