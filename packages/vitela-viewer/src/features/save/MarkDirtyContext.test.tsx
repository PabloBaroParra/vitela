import { renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MarkDirtyContext, useMarkDirty } from './MarkDirtyContext';

describe('useMarkDirty', () => {
  it('gives the workspace function to an editing screen', () => {
    const markDirty = vi.fn();
    const { result } = renderHook(() => useMarkDirty(), {
      wrapper: ({ children }) => <MarkDirtyContext.Provider value={markDirty}>{children}</MarkDirtyContext.Provider>,
    });
    result.current();
    expect(markDirty).toHaveBeenCalledOnce();
  });

  it('fails loudly outside a workspace instead of silently dropping edits', () => {
    expect(() => renderHook(() => useMarkDirty())).toThrow('needs the workspace');
  });
});
