import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { historyAction, historyKey, historyShortcuts, isApplePlatform } from './historyKeys';

const key = (init: Partial<KeyboardEventInit> & { key: string }) => new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });

afterEach(cleanup);

describe('historyKey', () => {
  it('reads Ctrl+Z as undo, Ctrl+Shift+Z and Ctrl+Y as redo, with ⌘ too', () => {
    expect(historyKey(key({ key: 'z', ctrlKey: true }), false)).toBe('undo');
    expect(historyKey(key({ key: 'z', metaKey: true }), false)).toBe('undo');
    expect(historyKey(key({ key: 'Z', ctrlKey: true, shiftKey: true }), false)).toBe('redo');
    expect(historyKey(key({ key: 'y', ctrlKey: true }), false)).toBe('redo');
  });

  it('leaves Y alone on a Mac, where ⌘+Y is the browser’s History', () => {
    expect(historyKey(key({ key: 'z', metaKey: true }), true)).toBe('undo');
    expect(historyKey(key({ key: 'z', metaKey: true, shiftKey: true }), true)).toBe('redo');
    expect(historyKey(key({ key: 'y', metaKey: true }), true)).toBeNull();
    expect(historyKey(key({ key: 'y', ctrlKey: true }), true)).toBeNull();
  });

  it('ignores other keys and Alt combinations', () => {
    expect(historyKey(key({ key: 'z' }), false)).toBeNull();
    expect(historyKey(key({ key: 'c', ctrlKey: true }), false)).toBeNull();
    expect(historyKey(key({ key: 'z', ctrlKey: true, altKey: true }), false)).toBeNull();
  });
});

describe('historyShortcuts', () => {
  it('names the keys historyKey reads, per platform', () => {
    expect(historyShortcuts(false)).toEqual({ undo: 'Control+Z', redo: 'Control+Shift+Z Control+Y' });
    expect(historyShortcuts(true)).toEqual({ undo: 'Meta+Z', redo: 'Meta+Shift+Z' });
  });
});

describe('isApplePlatform', () => {
  it('reads the platform, not the user agent', () => {
    expect(isApplePlatform({ platform: 'MacIntel', userAgent: '' })).toBe(true);
    expect(isApplePlatform({ platform: 'iPad', userAgent: '' })).toBe(true);
    expect(isApplePlatform({ platform: 'Win32', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' })).toBe(false);
    expect(isApplePlatform({ platform: 'Linux x86_64', userAgent: '' })).toBe(false);
  });

  it('falls back to the platform when the client hints leave theirs empty', () => {
    expect(isApplePlatform({ platform: 'MacIntel', userAgentData: { platform: '' } })).toBe(true);
    expect(isApplePlatform({ platform: 'Win32', userAgentData: { platform: 'macOS' } })).toBe(true);
  });
});

describe('historyAction', () => {
  it('leaves the keys to a text field', () => {
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
      expect(historyAction(event, false)).toBeNull();
    }
  });

  it('leaves a key someone else already took', () => {
    const taken = key({ key: 'z', ctrlKey: true });
    taken.preventDefault();
    document.body.dispatchEvent(taken);
    expect(historyAction(taken, false)).toBeNull();
  });

  it('takes nothing while a modal dialog is open, inside it or not', () => {
    const { container, unmount } = render(
      <dialog open aria-modal="true">
        <button type="button">Open</button>
      </dialog>,
    );
    const inside = key({ key: 'z', ctrlKey: true });
    container.querySelector('button')!.dispatchEvent(inside);
    expect(historyAction(inside, false)).toBeNull();
    const outside = key({ key: 'z', ctrlKey: true });
    document.body.dispatchEvent(outside);
    expect(historyAction(outside, false)).toBeNull();

    unmount();
    const after = key({ key: 'z', ctrlKey: true });
    document.body.dispatchEvent(after);
    expect(historyAction(after, false)).toBe('undo');
  });

  it("asks the editor's scope whether one of its dialogs is open, not the whole page", () => {
    const { unmount } = render(<dialog open aria-modal="true" />);
    const event = key({ key: 'z', ctrlKey: true });
    document.body.dispatchEvent(event);
    // A host page's dialog is not the editor's: the editor's scope sees none.
    expect(historyAction(event, false, { modalOpen: () => false })).toBe('undo');
    expect(historyAction(event, false, { modalOpen: () => true })).toBeNull();
    unmount();
  });
});
