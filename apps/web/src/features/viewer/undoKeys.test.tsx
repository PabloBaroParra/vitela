import { cleanup, render, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { historyKey, useUndoKeys } from './undoKeys';

const key = (init: Partial<KeyboardEventInit> & { key: string }) => new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });

afterEach(cleanup);

describe('historyKey', () => {
  it('reads Ctrl+Z as undo, Ctrl+Shift+Z and Ctrl+Y as redo, with ⌘ too', () => {
    expect(historyKey(key({ key: 'z', ctrlKey: true }))).toBe('undo');
    expect(historyKey(key({ key: 'z', metaKey: true }))).toBe('undo');
    expect(historyKey(key({ key: 'Z', ctrlKey: true, shiftKey: true }))).toBe('redo');
    expect(historyKey(key({ key: 'y', ctrlKey: true }))).toBe('redo');
  });

  it('ignores other keys and Alt combinations', () => {
    expect(historyKey(key({ key: 'z' }))).toBeNull();
    expect(historyKey(key({ key: 'c', ctrlKey: true }))).toBeNull();
    expect(historyKey(key({ key: 'z', ctrlKey: true, altKey: true }))).toBeNull();
  });
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
    const { container } = render(
      <div>
        <input aria-label="field" />
        <textarea aria-label="area" />
        <div contentEditable suppressContentEditableWarning>
          text
        </div>
      </div>,
    );

    for (const target of [...container.querySelectorAll('input, textarea, [contenteditable]')]) {
      const event = key({ key: 'z', ctrlKey: true });
      target.dispatchEvent(event);
      expect(event.defaultPrevented).toBe(false);
    }
    expect(onUndo).not.toHaveBeenCalled();
  });

  it('leaves a key someone else already took', () => {
    const onUndo = vi.fn();
    renderHook(() => useUndoKeys(onUndo, vi.fn()));
    const taken = key({ key: 'z', ctrlKey: true });
    taken.preventDefault();
    document.body.dispatchEvent(taken);
    expect(onUndo).not.toHaveBeenCalled();
  });
});
