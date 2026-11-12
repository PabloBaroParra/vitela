import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError } from '../../core/errors';
import type { PagePoint, TextSelection } from '../../core/types';
import { fakeEngine } from '../../test/fakeEngine';
import { useTextSelection } from './useTextSelection';

const selected = (text: string): TextSelection => ({ text, rects: new Float32Array([0, 0, text.length, 1]) });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe('useTextSelection', () => {
  it('selects from the press to the pointer on the page the press landed on', async () => {
    const calls: unknown[][] = [];
    const engine = fakeEngine({ selectText: async (...args) => (calls.push(args), selected('beta')) });
    const { result } = renderHook(() => useTextSelection(engine, 4, () => {}));

    act(() => result.current.input.start(2, { x: 10, y: 20 }));
    act(() => result.current.input.extend({ x: 90, y: 25 }));
    await waitFor(() => expect(result.current.selection?.text).toBe('beta'));
    expect(result.current.selection?.pageIndex).toBe(2);
    expect(calls).toEqual([[4, 2, { x: 10, y: 20 }, { x: 90, y: 25 }]]);
  });

  it('ignores moves when no press started a drag', () => {
    const selectText = vi.fn();
    const { result } = renderHook(() => useTextSelection(fakeEngine({ selectText }), 4, () => {}));
    act(() => result.current.input.extend({ x: 1, y: 1 }));
    act(() => result.current.input.start(0, { x: 0, y: 0 }));
    act(() => result.current.input.finish());
    act(() => result.current.input.extend({ x: 1, y: 1 }));
    expect(selectText).not.toHaveBeenCalled();
  });

  it('keeps one request in flight and sends only the latest of the moves made meanwhile', async () => {
    const answers: { focus: PagePoint; answer: ReturnType<typeof deferred<TextSelection>> }[] = [];
    const engine = fakeEngine({
      selectText: (_document, _page, _anchor, focus) => {
        const answer = deferred<TextSelection>();
        answers.push({ focus, answer });
        return answer.promise;
      },
    });
    const { result } = renderHook(() => useTextSelection(engine, 4, () => {}));
    act(() => result.current.input.start(0, { x: 0, y: 0 }));
    act(() => result.current.input.extend({ x: 1, y: 0 }));
    act(() => result.current.input.extend({ x: 2, y: 0 }));
    act(() => result.current.input.extend({ x: 3, y: 0 }));
    expect(answers.map(({ focus }) => focus.x)).toEqual([1]);

    // The first answer is stale by now: dropped, and the latest move is sent.
    await act(async () => answers[0]!.answer.resolve(selected('a')));
    expect(result.current.selection).toBeNull();
    expect(answers.map(({ focus }) => focus.x)).toEqual([1, 3]);
    await act(async () => answers[1]!.answer.resolve(selected('abc')));
    expect(result.current.selection?.text).toBe('abc');
  });

  it('a new press drops the old selection and any answer still coming for it', async () => {
    const late = deferred<TextSelection>();
    const engine = fakeEngine({ selectWord: () => late.promise });
    const { result } = renderHook(() => useTextSelection(engine, 4, () => {}));
    act(() => result.current.input.selectWord(1, { x: 5, y: 5 }));
    act(() => result.current.input.start(1, { x: 0, y: 0 }));
    await act(async () => late.resolve(selected('word')));
    expect(result.current.selection).toBeNull();
  });

  it('selects the word under a double click', async () => {
    const engine = fakeEngine({ selectWord: async () => selected('Vitela') });
    const { result } = renderHook(() => useTextSelection(engine, 4, () => {}));
    act(() => result.current.input.selectWord(3, { x: 5, y: 5 }));
    await waitFor(() => expect(result.current.selection).toMatchObject({ pageIndex: 3, text: 'Vitela' }));
    act(() => result.current.clear());
    expect(result.current.selection).toBeNull();
  });

  it('treats an empty answer as no selection', async () => {
    const engine = fakeEngine({ selectWord: async () => ({ text: '', rects: new Float32Array(0) }) });
    const { result } = renderHook(() => useTextSelection(engine, 4, () => {}));
    act(() => result.current.input.selectWord(0, { x: 5, y: 5 }));
    await act(async () => {});
    expect(result.current.selection).toBeNull();
  });

  it('reports TextExtractionDenied to the viewer', async () => {
    const onDenied = vi.fn();
    const engine = fakeEngine({
      selectWord: () => Promise.reject(new EngineError('TextExtractionDenied', 'text extraction is not permitted')),
    });
    const { result } = renderHook(() => useTextSelection(engine, 4, onDenied));
    act(() => result.current.input.selectWord(0, { x: 5, y: 5 }));
    await waitFor(() => expect(onDenied).toHaveBeenCalledOnce());
    expect(result.current.selection).toBeNull();
  });

  it('hands out the same input object across renders', () => {
    const engine = fakeEngine();
    // A new denial callback per render, as an inline arrow would be.
    const { result, rerender } = renderHook(() => useTextSelection(engine, 4, () => {}));
    const first = result.current.input;
    rerender();
    expect(result.current.input).toBe(first);
  });
});
