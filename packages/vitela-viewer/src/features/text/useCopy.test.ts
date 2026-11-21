import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { isCopyShortcut, useCopy } from './useCopy';

const key = (init: KeyboardEventInit, target: EventTarget = document.body) => {
  const event = new KeyboardEvent('keydown', { key: 'c', bubbles: true, cancelable: true, ...init });
  Object.defineProperty(event, 'target', { value: target });
  return event;
};

const collapsed = { isCollapsed: true } as Selection;

describe('isCopyShortcut', () => {
  it('answers Ctrl+C and ⌘+C', () => {
    expect(isCopyShortcut(key({ ctrlKey: true }), collapsed)).toBe(true);
    expect(isCopyShortcut(key({ metaKey: true, key: 'C' }), null)).toBe(true);
  });

  it('leaves other keys and chords alone', () => {
    expect(isCopyShortcut(key({}), collapsed)).toBe(false);
    expect(isCopyShortcut(key({ ctrlKey: true, key: 'v' }), collapsed)).toBe(false);
    expect(isCopyShortcut(key({ ctrlKey: true, shiftKey: true }), collapsed)).toBe(false);
    expect(isCopyShortcut(key({ ctrlKey: true, altKey: true }), collapsed)).toBe(false);
  });

  it('leaves a copy typed in a field to the field', () => {
    expect(isCopyShortcut(key({ ctrlKey: true }, document.createElement('input')), collapsed)).toBe(false);
    expect(isCopyShortcut(key({ ctrlKey: true }, document.createElement('textarea')), collapsed)).toBe(false);
  });

  it('leaves text the browser has selected to the browser', () => {
    expect(isCopyShortcut(key({ ctrlKey: true }), { isCollapsed: false } as Selection)).toBe(false);
  });
});

describe('useCopy', () => {
  afterEach(() => {
    // Unmount: a hook left mounted keeps its key listener on the window.
    cleanup();
    vi.unstubAllGlobals();
  });

  function stubClipboard(writeText: (text: string) => Promise<void>) {
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });
  }

  it('writes the selection to the clipboard on Ctrl+C and reports it', async () => {
    const written: string[] = [];
    stubClipboard(async (text) => {
      written.push(text);
    });
    const { result } = renderHook(() => useCopy('Hello Vitela'));
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', ctrlKey: true, cancelable: true }));
    });
    await waitFor(() => expect(result.current.status).toBe('copied'));
    expect(written).toEqual(['Hello Vitela']);
  });

  it('copies nothing, and takes no key, with nothing selected', () => {
    const writeText = vi.fn();
    stubClipboard(writeText);
    const { result } = renderHook(() => useCopy(null));
    const event = new KeyboardEvent('keydown', { key: 'c', ctrlKey: true, cancelable: true });
    act(() => {
      window.dispatchEvent(event);
      result.current.copy();
    });
    expect(event.defaultPrevented).toBe(false);
    expect(writeText).not.toHaveBeenCalled();
  });

  it('reports a refused write, and starts over for a new selection', async () => {
    stubClipboard(() => Promise.reject(new Error('denied')));
    const { result, rerender } = renderHook(({ text }) => useCopy(text), { initialProps: { text: 'one' } });
    act(() => result.current.copy());
    await waitFor(() => expect(result.current.status).toBe('failed'));
    rerender({ text: 'two' });
    expect(result.current.status).toBe('idle');
  });
});
