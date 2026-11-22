import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { WritableFileHandle } from '../../platform/fileHandle';
import type { EditorScope } from './editorScope';
import { useFileDrop } from './useFileDrop';

const pdf = new File([new Uint8Array([37, 80, 68, 70])], 'dropped.pdf', { type: 'application/pdf' });

/** A cancelable drag event carrying `files` (jsdom has no DragEvent/DataTransfer). */
function drag(type: string, files: File[] = [pdf], items?: unknown[], target: EventTarget = window): Event {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'dataTransfer', { value: { types: ['Files'], files, items, dropEffect: 'none' } });
  act(() => {
    target.dispatchEvent(event);
  });
  return event;
}

// Window listeners outlive a test unless its hook is unmounted.
afterEach(() => {
  cleanup();
  document.body.replaceChildren();
});

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

  it('ends the drag but opens nothing when the page already claimed the drop', () => {
    const opened: File[] = [];
    const { result } = renderHook(() => useFileDrop((file) => opened.push(file), true));
    drag('dragenter');
    const event = new Event('drop', { bubbles: true, cancelable: true });
    Object.defineProperty(event, 'dataTransfer', { value: { types: ['Files'], files: [pdf], dropEffect: 'none' } });
    event.preventDefault();
    act(() => {
      window.dispatchEvent(event);
    });
    expect(opened).toEqual([]);
    expect(result.current).toBe(false);
  });

  it('takes only the drags over the editor when its scope says so, leaving the rest to the page', () => {
    const editor = document.body.appendChild(document.createElement('div'));
    const host = document.body.appendChild(document.createElement('div'));
    const scope: EditorScope = {
      ownsEvent: () => true,
      takesDrop: (target) => target instanceof Node && editor.contains(target),
      modalOpen: () => false,
    };
    const opened: File[] = [];
    const { result } = renderHook(() => useFileDrop((file) => opened.push(file), true, scope));

    // Over the host: no hint, the browser's default left alone, nothing opened.
    drag('dragenter', [pdf], undefined, host);
    expect(result.current).toBe(false);
    expect(drag('dragover', [pdf], undefined, host).defaultPrevented).toBe(false);
    expect(drag('drop', [pdf], undefined, host).defaultPrevented).toBe(false);
    expect(opened).toEqual([]);

    // Into the editor and out again: the hint follows.
    drag('dragenter', [pdf], undefined, editor);
    expect(result.current).toBe(true);
    drag('dragenter', [pdf], undefined, host);
    drag('dragleave', [pdf], undefined, editor);
    expect(result.current).toBe(false);

    // Dropped over the editor: opened.
    drag('dragenter', [pdf], undefined, editor);
    expect(drag('dragover', [pdf], undefined, editor).defaultPrevented).toBe(true);
    expect(drag('drop', [pdf], undefined, editor).defaultPrevented).toBe(true);
    expect(opened).toEqual([pdf]);
    expect(result.current).toBe(false);
  });

  it('counts a drag leaving an element that was taken out of the page, so the hint goes', () => {
    const editor = document.body.appendChild(document.createElement('div'));
    const child = editor.appendChild(document.createElement('span'));
    const scope: EditorScope = {
      ownsEvent: () => true,
      takesDrop: (target) => target instanceof Node && editor.contains(target),
      modalOpen: () => false,
    };
    const { result } = renderHook(() => useFileDrop(() => {}, true, scope));
    drag('dragenter', [pdf], undefined, child);
    expect(result.current).toBe(true);
    // Something under the drag takes the element out while its dragleave is
    // on the way: the event still reaches the window, from a detached target.
    child.addEventListener('dragleave', () => child.remove());
    drag('dragleave', [pdf], undefined, child);
    expect(child.isConnected).toBe(false);
    expect(result.current).toBe(false);
  });

  it('leaves drags that carry no files to the browser', () => {
    renderHook(() => useFileDrop(() => {}, false));
    const event = new Event('dragover', { cancelable: true });
    Object.defineProperty(event, 'dataTransfer', { value: { types: ['text/plain'], files: [] } });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });

  it('hands over the file handle of the dropped file when the browser gives one', async () => {
    const handle = { kind: 'file', name: 'dropped.pdf', createWritable: async () => ({}) } as unknown as WritableFileHandle;
    const other = new File([new Uint8Array([1])], 'notes.txt', { type: 'text/plain' });
    const opened: [File, WritableFileHandle | undefined][] = [];
    renderHook(() => useFileDrop((file, fileHandle) => opened.push([file, fileHandle]), true));
    // The PDF is the second file: its handle must be the second item's.
    drag('drop', [other, pdf], [
      { kind: 'file', getAsFileSystemHandle: async () => ({ kind: 'file' }) },
      { kind: 'file', getAsFileSystemHandle: async () => handle },
    ]);
    await act(async () => {});
    expect(opened).toEqual([[pdf, handle]]);
  });

  it('opens the file without a handle when the browser gives none for it', async () => {
    const opened: [File, WritableFileHandle | undefined][] = [];
    renderHook(() => useFileDrop((file, fileHandle) => opened.push([file, fileHandle]), true));
    drag('drop', [pdf], [{ kind: 'file', getAsFileSystemHandle: async () => null }]);
    await act(async () => {});
    expect(opened).toEqual([[pdf, undefined]]);
  });

  describe('a handle that arrives after the drop', () => {
    const handleOf = (name: string) =>
      ({ kind: 'file', name, createWritable: async () => ({}) }) as unknown as WritableFileHandle;
    const second = new File([new Uint8Array([37])], 'second.pdf', { type: 'application/pdf' });

    /** An item whose handle only arrives when `release` is called. */
    function slowItem(handle: WritableFileHandle | null) {
      let release: () => void = () => {};
      const gate = new Promise<void>((resolve) => (release = resolve));
      return { item: { kind: 'file', getAsFileSystemHandle: async () => (await gate, handle) }, release };
    }

    it('ignores a drop that became disabled while its handle was pending', async () => {
      const opened: File[] = [];
      const { rerender } = renderHook(({ enabled }) => useFileDrop((file) => opened.push(file), enabled), {
        initialProps: { enabled: true },
      });
      const slow = slowItem(handleOf('dropped.pdf'));
      drag('drop', [pdf], [slow.item]);
      // Another open started meanwhile (the busy check passed before the await).
      rerender({ enabled: false });
      slow.release();
      await act(async () => {});
      expect(opened).toEqual([]);
    });

    it('lets only the latest drop win', async () => {
      const opened: File[] = [];
      renderHook(() => useFileDrop((file) => opened.push(file), true));
      const slow = slowItem(handleOf('dropped.pdf'));
      drag('drop', [pdf], [slow.item]);
      drag('drop', [second], [{ kind: 'file', getAsFileSystemHandle: async () => handleOf('second.pdf') }]);
      await act(async () => {});
      slow.release();
      await act(async () => {});
      expect(opened).toEqual([second]);
    });

    it('opens without the handle when it belongs to another file, so Save downloads', async () => {
      const opened: [File, WritableFileHandle | undefined][] = [];
      renderHook(() => useFileDrop((file, fileHandle) => opened.push([file, fileHandle]), true));
      drag('drop', [pdf], [{ kind: 'file', getAsFileSystemHandle: async () => handleOf('other.pdf') }]);
      await act(async () => {});
      expect(opened).toEqual([[pdf, undefined]]);
    });
  });
});
