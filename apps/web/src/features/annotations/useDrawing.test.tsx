import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ENCODING_GAP } from '@vitela/core';
import type { AnnotationEdit } from '@vitela/core';
import { NOTE_SIZE, PALETTE } from './drawing';
import type { AddOutcome } from './useAnnotationEdits';
import { useDrawing, type DrawingEdits } from './useDrawing';

const BLUE = PALETTE.find(({ name }) => name === 'Blue')!.color;
const BOX = { left: 100, top: 100, width: 200, height: 50 };

afterEach(() => {
  document.body.innerHTML = '';
});

function drawing(outcome: AddOutcome = { status: 'added' }, allowed = true, rotation = 0) {
  const add = vi.fn(async (_edit: AnnotationEdit, _page: number, _key: string, _shown?: readonly string[]) => outcome);
  const edits: DrawingEdits = { allowed, add };
  const view = renderHook((props: DrawingEdits) => useDrawing(props, () => rotation), { initialProps: edits });
  return { ...view, add };
}

describe('useDrawing', () => {
  it('starts on Select in the default colour; Escape goes back to Select', () => {
    const { result } = drawing();
    expect(result.current.tool).toBe('select');
    expect(result.current.color).toEqual(PALETTE[0]!.color);
    act(() => result.current.choose('ink'));
    expect(result.current.tool).toBe('ink');
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(result.current.tool).toBe('select');
  });

  it('leaves Escape to a text field or an open dialog', () => {
    const { result } = drawing();
    act(() => result.current.choose('shape'));
    const input = document.body.appendChild(document.createElement('input'));
    act(() => {
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    });
    expect(result.current.tool).toBe('shape');
    document.body.appendChild(document.createElement('dialog')).setAttribute('open', '');
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(result.current.tool).toBe('shape');
  });

  it('arms nothing, and drops an armed tool, when annotating is forbidden', () => {
    const { result, rerender, add } = drawing();
    act(() => result.current.choose('note'));
    rerender({ allowed: false, add });
    expect(result.current.tool).toBe('select');
    act(() => result.current.choose('ink'));
    expect(result.current.tool).toBe('select');
  });

  it('adds a stroke or a shape at once, in the chosen colour, for the page it began on', () => {
    const { result, add } = drawing();
    act(() => result.current.setColor(BLUE));
    const points = [
      { x: 1, y: 2 },
      { x: 3, y: 4 },
    ];
    act(() => result.current.onGesture(2, '5/0', 'ink', { kind: 'stroke', points }));
    act(() => result.current.onGesture(2, '5/0', 'shape', { kind: 'box', rect: BOX }));
    expect(add.mock.calls).toEqual([
      [{ kind: 'addInk', page: 2, points, color: BLUE }, 2, '5/0'],
      [{ kind: 'addShape', page: 2, rect: BOX, color: BLUE }, 2, '5/0'],
    ]);
    expect(result.current.draft).toBeNull();
  });

  it('asks for a note’s text before adding it, and closes once it is in', async () => {
    const { result, add } = drawing();
    const rect = { left: 10, top: 10, ...NOTE_SIZE };
    act(() => result.current.onGesture(0, '0/0', 'note', { kind: 'box', rect }));
    expect(add).not.toHaveBeenCalled();
    expect(result.current.draft).toMatchObject({ tool: 'note', pageIndex: 0, rect, busy: false, error: null });

    await act(() => result.current.submit('Remember this'));
    expect(add).toHaveBeenCalledWith({ kind: 'addNote', page: 0, rect, contents: 'Remember this' }, 0, '0/0', [ENCODING_GAP]);
    expect(result.current.draft).toBeNull();
  });

  it('keeps a text box open with the core’s message when it cannot show a character', async () => {
    const message = "'\u{1F600}' cannot be shown in Helvetica (WinAnsi)";
    const { result } = drawing({ status: 'failed', kind: ENCODING_GAP, message });
    act(() => result.current.onGesture(0, '0/0', 'freeText', { kind: 'box', rect: BOX }));
    await act(() => result.current.submit('smile \u{1F600}'));
    expect(result.current.draft).toMatchObject({ tool: 'freeText', busy: false, error: message });
    // Typing again clears it.
    act(() => result.current.retyped());
    expect(result.current.draft?.error).toBeNull();
  });

  it('closes on any other failure (the viewer shows it) and on cancel', async () => {
    const { result } = drawing({ status: 'failed', kind: null, message: 'The page changed' });
    act(() => result.current.onGesture(0, '0/0', 'freeText', { kind: 'box', rect: BOX }));
    await act(() => result.current.submit('hello'));
    expect(result.current.draft).toBeNull();

    act(() => result.current.onGesture(0, '0/0', 'note', { kind: 'box', rect: BOX }));
    act(() => result.current.cancel());
    expect(result.current.draft).toBeNull();
  });

  it('lays a text box out in its own frame: sides swapped on a quarter-turned page', () => {
    const { result } = drawing({ status: 'added' }, true, 90);
    act(() => result.current.onGesture(0, '0/0', 'freeText', { kind: 'box', rect: BOX }));
    expect(result.current.draft?.frame).toEqual({ width: 50, height: 200 });
  });
});
