import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { useFileDrop } from './useFileDrop';

const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'dropped.pdf', { type: 'application/pdf' });

/** A cancelable drag event carrying `files` (jsdom has no DragEvent/DataTransfer). */
function drag(type: string, files: File[] = [pdf]): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'dataTransfer', { value: { types: ['Files'], files, dropEffect: 'none' } });
  act(() => {
    window.dispatchEvent(event);
  });
  return event;
}

// Window listeners outlive a test unless its hook is unmounted.
afterEach(cleanup);

describe('useFileDrop', () => {
  it('hands over a dropped file and reports the drag while enabled', () => {
    const opened: File[] = [];
    const { result } = renderHook(() => useFileDrop((file) => opened.push(file), true));
    drag('dragenter');
    expect(result.current).toBe(true);
    expect(drag('dragover').defaultPrevented).toBe(true);
    expect(drag('drop').defaultPrevented).toBe(true);
    expect(opened).toEqual([pdf]);
    expect(result.current).toBe(false);
  });

  it('while disabled, still stops the browser opening a dropped file, and ignores it', () => {
    const opened: File[] = [];
    const { result } = renderHook(() => useFileDrop((file) => opened.push(file), false));
    drag('dragenter');
    expect(result.current).toBe(false);
    // Without preventDefault the browser navigates to the file and unloads the app.
    expect(drag('dragover').defaultPrevented).toBe(true);
    expect(drag('drop').defaultPrevented).toBe(true);
    expect(opened).toEqual([]);
  });

  it('leaves drags that carry no files to the browser', () => {
    renderHook(() => useFileDrop(() => {}, false));
    const event = new Event('dragover', { cancelable: true });
    Object.defineProperty(event, 'dataTransfer', { value: { types: ['text/plain'], files: [] } });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });
});
