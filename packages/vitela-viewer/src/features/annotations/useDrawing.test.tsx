import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ENCODING_GAP } from '@vitela/core';
import { NOTE_SIZE, PALETTE } from './drawing';
import type { AddOutcome, PendingEdit } from './useAnnotationEdits';
import type { EditorScope } from '../document/editorScope';
import { useDrawing, type DrawingEdits } from './useDrawing';

const BLUE = PALETTE.find(({ name }) => name === 'Blue')!.color;
const BOX = { left: 100, top: 100, width: 200, height: 50 };

afterEach(() => {
  document.body.innerHTML = '';
});

function drawing(
  outcome: AddOutcome | Promise<AddOutcome> = { status: 'added' },
  allowed = true,
  rotation = 0,
  scope?: EditorScope,
) {
  const add = vi.fn(async (_edit: PendingEdit, _page: number, _key: string, _shown?: readonly string[]) => outcome);
  const notify = vi.fn();
  const edits: DrawingEdits = { allowed, add, notify };
  const onStamp = vi.fn();
  // Each pick waits until the test says whether the image read.
  const picks: ((read: boolean) => void)[] = [];
  const pick = vi.fn((_file: File) => new Promise<boolean>((resolve) => picks.push(resolve)));
  const view = renderHook((props: DrawingEdits) => useDrawing(props, () => rotation, { place: onStamp, pick }, scope), {
    initialProps: edits,
  });
  return { ...view, add, notify, onStamp, pick, picks };
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

  it("leaves Escape pressed outside the editor, and goes by the editor's own dialogs", () => {
    let inside = false;
    let modal = false;
    const scope: EditorScope = { ownsEvent: () => inside, takesDrop: () => true, modalOpen: () => modal };
    const { result } = drawing(undefined, true, 0, scope);
    act(() => result.current.choose('ink'));
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(result.current.tool).toBe('ink');
    // A host dialog open elsewhere on the page does not hold the editor's Escape.
    inside = true;
    document.body.appendChild(document.createElement('dialog')).setAttribute('open', '');
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(result.current.tool).toBe('select');
    act(() => result.current.choose('ink'));
    modal = true;
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    expect(result.current.tool).toBe('ink');
  });

  it('arms nothing, and drops an armed tool, when annotating is forbidden', () => {
    const { result, rerender, add } = drawing();
    act(() => result.current.choose('note'));
    rerender({ allowed: false, add, notify: vi.fn() });
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

  it('hands a stamp click over as the anchor for the page it began on, with no draft', () => {
    const { result, add, onStamp } = drawing();
    act(() => result.current.choose('stamp'));
    expect(result.current.tool).toBe('stamp');
    act(() => result.current.onGesture(1, '3/90', 'stamp', { kind: 'point', point: { x: 40, y: 50 } }));
    expect(onStamp).toHaveBeenCalledWith(1, '3/90', { x: 40, y: 50 });
    expect(add).not.toHaveBeenCalled();
    expect(result.current.draft).toBeNull();
  });

  it('arms Image once the picked image reads, unless the tool changed or another pick began meanwhile', async () => {
    const image = new File([], 'a.png');
    const { result, pick, picks } = drawing();

    act(() => result.current.pickImage(image));
    expect(pick).toHaveBeenCalledWith(image);
    await act(async () => picks[0]!(true));
    expect(result.current.tool).toBe('stamp');

    // The user chose Ink while the image was loading: Ink stays.
    act(() => result.current.choose('select'));
    act(() => result.current.pickImage(image));
    act(() => result.current.choose('ink'));
    await act(async () => picks[1]!(true));
    expect(result.current.tool).toBe('ink');

    // Escape while loading counts as a choice too.
    act(() => result.current.pickImage(image));
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    });
    await act(async () => picks[2]!(true));
    expect(result.current.tool).toBe('select');

    // A pick superseded by a later one arms nothing; one that did not read arms nothing.
    act(() => result.current.pickImage(image));
    act(() => result.current.pickImage(image));
    await act(async () => picks[4]!(false));
    await act(async () => picks[3]!(true));
    expect(result.current.tool).toBe('select');
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

  it('drops the core’s refusal of text sent when the text was retyped before it answered', async () => {
    const message = "'\u{65E5}' cannot be shown in Helvetica (WinAnsi)";
    let answer: (outcome: AddOutcome) => void = () => {};
    const { result } = drawing(new Promise<AddOutcome>((resolve) => (answer = resolve)));
    act(() => result.current.onGesture(0, '0/0', 'freeText', { kind: 'box', rect: BOX }));
    let sent: Promise<void> = Promise.resolve();
    act(() => {
      sent = result.current.submit('Total \u{65E5}');
    });
    expect(result.current.draft?.busy).toBe(true);
    // Retyped while the core is still answering for the old text.
    act(() => result.current.retyped());
    await act(async () => {
      answer({ status: 'failed', kind: ENCODING_GAP, message });
      await sent;
    });
    expect(result.current.draft).toMatchObject({ tool: 'freeText', busy: false, error: null });
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

  it('tells the user when a stroke or a shape is refused (a step started mid-gesture)', async () => {
    const { result, notify } = drawing({ status: 'refused' });
    const points = [
      { x: 1, y: 2 },
      { x: 3, y: 4 },
    ];
    await act(async () => result.current.onGesture(0, '0/0', 'ink', { kind: 'stroke', points }));
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify.mock.calls[0]![0]).toMatch(/nothing was added/i);
    await act(async () => result.current.onGesture(0, '0/0', 'shape', { kind: 'box', rect: BOX }));
    expect(notify).toHaveBeenCalledTimes(2);
  });

  it('says nothing more when a stroke is added, or failed (the viewer shows that failure)', async () => {
    const added = drawing({ status: 'added' });
    const points = [
      { x: 1, y: 2 },
      { x: 3, y: 4 },
    ];
    await act(async () => added.result.current.onGesture(0, '0/0', 'ink', { kind: 'stroke', points }));
    expect(added.notify).not.toHaveBeenCalled();
    const failed = drawing({ status: 'failed', kind: null, message: 'The page changed' });
    await act(async () => failed.result.current.onGesture(0, '0/0', 'ink', { kind: 'stroke', points }));
    expect(failed.notify).not.toHaveBeenCalled();
  });

  it('keeps the dialog open on cancel while its text is being added', async () => {
    let answer: (outcome: AddOutcome) => void = () => {};
    const { result } = drawing(new Promise<AddOutcome>((resolve) => (answer = resolve)));
    act(() => result.current.onGesture(0, '0/0', 'note', { kind: 'box', rect: BOX }));
    let sent: Promise<void> = Promise.resolve();
    act(() => {
      sent = result.current.submit('hello');
    });
    expect(result.current.draft?.busy).toBe(true);
    act(() => result.current.cancel());
    expect(result.current.draft).toMatchObject({ busy: true });
    await act(async () => {
      answer({ status: 'added' });
      await sent;
    });
    expect(result.current.draft).toBeNull();
  });
});
