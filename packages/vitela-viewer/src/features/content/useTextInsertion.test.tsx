import { act, cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Edit } from '@vitela/core';
import type { Session } from '../annotations/useAnnotationEdits';
import { INSERT_TEXT_PROMPT, INSERT_TEXT_SIZE_PT } from './textBlocks';
import { TextInsertEditor } from './TextInsertEditor';
import type { ContentMode } from './useContentMode';
import { useTextInsertion } from './useTextInsertion';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const info = {
  pages: [
    { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
    { widthPt: 792, heightPt: 612, rotationDegrees: 90 },
  ],
  pageIds: [9, 11],
};
const session: Session = { info, annotations: [] };

function setup() {
  const applied: (Edit | null)[] = [];
  const errors: unknown[] = [];
  const follows: unknown[][] = [];
  /** The session a step sees when its turn comes. */
  const turn = { session };
  const content = { active: true, follow: (...args: unknown[]) => void follows.push(args) } as unknown as ContentMode;
  const queue = {
    pending: false,
    apply: async (build: (session: Session) => Edit | null) => {
      try {
        applied.push(build(turn.session));
      } catch (error) {
        errors.push(error);
      }
    },
    notify: () => {},
  };
  const focusPages = vi.fn();
  const hook = renderHook(({ active }: { active: boolean }) => useTextInsertion({ info, content: { ...content, active }, queue, focusPages }), {
    initialProps: { active: true },
  });
  return { hook, applied, errors, follows, focusPages, turn };
}

describe('useTextInsertion', () => {
  it('takes no press until it is armed', () => {
    const { hook } = setup();
    let placed = true;
    act(() => void (placed = hook.result.current.place(0, { x: 10, y: 10 })));
    expect(placed).toBe(false);
    expect(hook.result.current.editor).toBeNull();
    expect(hook.result.current.prompt).toBeNull();
  });

  it('arms, says how to place, and opens an empty editor where the page is pressed', () => {
    const { hook, focusPages } = setup();
    act(() => hook.result.current.arm());
    expect(hook.result.current.armed).toBe(true);
    expect(hook.result.current.prompt).toBe(INSERT_TEXT_PROMPT);
    // The button keeps the focus otherwise, and Escape is the pages'.
    expect(focusPages).toHaveBeenCalled();

    let placed = false;
    act(() => void (placed = hook.result.current.place(0, { x: 100, y: 200 })));
    expect(placed).toBe(true);
    expect(hook.result.current.armed).toBe(false);
    expect(hook.result.current.editor).toMatchObject({ pageIndex: 0, at: { x: 100, y: 200 }, draft: '' });
  });

  it('records what was typed as one insertion and follows the new line to where it lands', () => {
    vi.useFakeTimers();
    const { hook, applied, follows } = setup();
    act(() => hook.result.current.arm());
    act(() => void hook.result.current.place(0, { x: 100, y: 200 }));
    act(() => hook.result.current.typing('Added'));
    act(() => hook.result.current.commit(true));
    expect(applied).toEqual([{ kind: 'insertText', page: 0, at: { x: 100, y: 200 }, text: 'Added', sizePt: INSERT_TEXT_SIZE_PT }]);
    expect(follows).toHaveLength(1);
    expect(follows[0]![0]).toBe(0);
    expect(follows[0]![1]).toMatchObject({ left: 100, top: 200, height: INSERT_TEXT_SIZE_PT });
    expect(follows[0]![2]).toBe('Added');
    act(() => void vi.runAllTimers());
    expect(hook.result.current.editor).toBeNull();
  });

  it('records nothing for an editor left empty, or cancelled', () => {
    vi.useFakeTimers();
    const { hook, applied } = setup();
    act(() => hook.result.current.arm());
    act(() => void hook.result.current.place(0, { x: 100, y: 200 }));
    act(() => hook.result.current.commit(false));
    act(() => void vi.runAllTimers());
    act(() => hook.result.current.arm());
    act(() => void hook.result.current.place(0, { x: 100, y: 200 }));
    act(() => hook.result.current.typing('Gone'));
    act(() => hook.result.current.cancel());
    expect(applied).toEqual([]);
  });

  it('disarms on Escape at the pages, and when the mode is turned off', () => {
    const { hook } = setup();
    act(() => hook.result.current.arm());
    let used = false;
    act(() => void (used = hook.result.current.onKey('Escape')));
    expect(used).toBe(true);
    expect(hook.result.current.armed).toBe(false);
    act(() => void (used = hook.result.current.onKey('Escape')));
    expect(used).toBe(false);

    act(() => hook.result.current.arm());
    hook.rerender({ active: false });
    expect(hook.result.current.armed).toBe(false);
  });

  it('commits a draft when the mode is turned off', () => {
    const { hook, applied } = setup();
    act(() => hook.result.current.arm());
    act(() => void hook.result.current.place(1, { x: 300, y: 40 }));
    act(() => hook.result.current.typing('Kept'));
    hook.rerender({ active: false });
    expect(applied).toEqual([{ kind: 'insertText', page: 1, at: { x: 300, y: 40 }, text: 'Kept', sizePt: INSERT_TEXT_SIZE_PT }]);
  });

  it('refuses the step when the page at the position changed before its turn', () => {
    const { hook, errors, turn } = setup();
    act(() => hook.result.current.arm());
    act(() => void hook.result.current.place(0, { x: 100, y: 200 }));
    act(() => hook.result.current.typing('Late'));
    // The step's turn comes once the page at position 0 is another one.
    turn.session = { info: { ...info, pageIds: [10, 11] }, annotations: [] };
    act(() => hook.result.current.commit(false));
    expect(String(errors[0])).toMatch(/page changed/i);
  });
});

describe('TextInsertEditor', () => {
  const editor = { pageIndex: 0, at: { x: 100, y: 200 }, pageKey: '9/0', draft: '' };

  it('hangs from the point in Helvetica at the insertion size, turned with the page', () => {
    render(<TextInsertEditor editor={editor} scale={2} rotationDegrees={90} onCommit={() => {}} onCancel={() => {}} onType={() => {}} onAttach={() => {}} onDetach={() => {}} />);
    const field = screen.getByRole('textbox', { name: 'New text' });
    expect(field.style.left).toBe('200px');
    expect(field.style.top).toBe('400px');
    expect(field.style.fontSize).toBe(`${INSERT_TEXT_SIZE_PT * 2}px`);
    expect(field.style.fontFamily).toContain('Helvetica');
    expect(field.style.transform).toBe('rotate(90deg)');
    expect(document.activeElement).toBe(field);
  });

  it('commits on Enter and on leaving it, cancels on Escape, and keeps the pages’ keys off', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const onType = vi.fn();
    const pages = vi.fn();
    render(
      <div onKeyDown={pages}>
        <TextInsertEditor editor={editor} scale={1} rotationDegrees={0} onCommit={onCommit} onCancel={onCancel} onType={onType} onAttach={() => {}} onDetach={() => {}} />
      </div>,
    );
    const field = screen.getByRole('textbox', { name: 'New text' });
    fireEvent.change(field, { target: { value: 'Hi' } });
    expect(onType).toHaveBeenCalledWith('Hi');
    fireEvent.keyDown(field, { key: 'ArrowLeft' });
    expect(pages).not.toHaveBeenCalled();
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(onCommit).toHaveBeenCalledWith(true);
    fireEvent.keyDown(field, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalled();
    fireEvent.blur(field);
    expect(onCommit).toHaveBeenLastCalledWith(false);
  });
});
