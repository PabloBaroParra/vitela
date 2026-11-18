import { cleanup, render, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useUndoKeys } from './useUndoKeys';

const key = (init: Partial<KeyboardEventInit> & { key: string }) => new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('useUndoKeys', () => {
  it('undoes and redoes from anywhere on the page, taking the key', () => {
    const onUndo = vi.fn();
    const onRedo = vi.fn();
    renderHook(() => useUndoKeys(onUndo, onRedo));

    const undo = key({ key: 'z', ctrlKey: true });
    document.body.dispatchEvent(undo);
    document.body.dispatchEvent(key({ key: 'y', ctrlKey: true }));
    expect(onUndo).toHaveBeenCalledTimes(1);
    expect(onRedo).toHaveBeenCalledTimes(1);
    expect(undo.defaultPrevented).toBe(true);
  });

  it('leaves the keys to a text field', () => {
    const onUndo = vi.fn();
    renderHook(() => useUndoKeys(onUndo, vi.fn()));
    const { container } = render(<input aria-label="field" />);
    const event = key({ key: 'z', ctrlKey: true });
    container.querySelector('input')!.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
    expect(onUndo).not.toHaveBeenCalled();
  });

  it('does nothing behind a modal dialog', () => {
    const onUndo = vi.fn();
    const onRedo = vi.fn();
    renderHook(() => useUndoKeys(onUndo, onRedo));
    const { container } = render(
      <dialog open aria-modal="true">
        <button type="button">Cancel</button>
      </dialog>,
    );
    const event = key({ key: 'z', ctrlKey: true });
    container.querySelector('button')!.dispatchEvent(event);
    document.body.dispatchEvent(key({ key: 'Z', ctrlKey: true, shiftKey: true }));
    expect(event.defaultPrevented).toBe(false);
    expect(onUndo).not.toHaveBeenCalled();
    expect(onRedo).not.toHaveBeenCalled();
  });

  it('leaves ⌘+Y to the browser on a Mac', () => {
    vi.stubGlobal('navigator', { ...navigator, platform: 'MacIntel', userAgent: navigator.userAgent });
    const onRedo = vi.fn();
    renderHook(() => useUndoKeys(vi.fn(), onRedo));
    const history = key({ key: 'y', metaKey: true });
    document.body.dispatchEvent(history);
    expect(onRedo).not.toHaveBeenCalled();
    expect(history.defaultPrevented).toBe(false);
    document.body.dispatchEvent(key({ key: 'z', metaKey: true, shiftKey: true }));
    expect(onRedo).toHaveBeenCalledTimes(1);
  });
});
