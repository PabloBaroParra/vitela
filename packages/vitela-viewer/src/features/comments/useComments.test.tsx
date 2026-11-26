import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Comment, Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { useComments } from './useComments';

const comment = (contents: string): Comment => ({
  pageIndex: 0,
  rect: { left: 0, top: 0, width: 1, height: 1 },
  contents,
  author: null,
  date: null,
  annotationId: null,
});

function deferred() {
  let resolve!: (value: Comment[]) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<Comment[]>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function setup() {
  const asked: ReturnType<typeof deferred>[] = [];
  const comments = vi.fn(() => {
    const answer = deferred();
    asked.push(answer);
    return answer.promise;
  });
  const engine: Engine = fakeEngine({ comments });
  const view = renderHook(({ revision, structure }) => useComments(engine, 1, revision, structure), {
    initialProps: { revision: {} as object, structure: 'p' },
  });
  const shown = () => {
    const { state } = view.result.current;
    return [state.status, state.comments.map(({ contents }) => contents)];
  };
  const settle = (index: number, ...texts: string[]) => act(async () => asked[index]!.resolve(texts.map(comment)));
  return { ...view, asked, shown, settle };
}

describe('useComments', () => {
  it('loads, then keeps the list shown while an edit refreshes it', async () => {
    const { shown, settle, rerender, asked } = setup();
    expect(shown()).toEqual(['loading', []]);
    await settle(0, 'one');
    expect(shown()).toEqual(['ready', ['one']]);
    rerender({ revision: {}, structure: 'p' });
    expect(asked).toHaveLength(2);
    expect(shown()).toEqual(['ready', ['one']]);
    await settle(1, 'one', 'two');
    expect(shown()).toEqual(['ready', ['one', 'two']]);
  });

  it('asks once more, not once per edit, for edits made while it is being answered', async () => {
    const { shown, settle, rerender, asked } = setup();
    for (let i = 0; i < 5; i++) rerender({ revision: {}, structure: 'p' });
    expect(asked).toHaveLength(1);
    await settle(0, 'stale');
    expect(shown()).toEqual(['loading', []]);
    expect(asked).toHaveLength(2);
    await settle(1, 'fresh');
    expect(shown()).toEqual(['ready', ['fresh']]);
    expect(asked).toHaveLength(2);
  });

  it('shows no comments of the previous pages while other pages are there', async () => {
    const { shown, settle, rerender } = setup();
    await settle(0, 'one');
    rerender({ revision: {}, structure: 'q' });
    expect(shown()).toEqual(['loading', []]);
    await settle(1, 'two');
    expect(shown()).toEqual(['ready', ['two']]);
  });

  it('says why it failed and tries again when asked', async () => {
    const { result, shown, settle, asked } = setup();
    await act(async () => asked[0]!.reject(new Error('broken')));
    expect(shown()).toEqual(['failed', []]);
    expect(result.current.state).toMatchObject({ message: 'broken' });
    act(() => result.current.retry());
    expect(shown()).toEqual(['loading', []]);
    await settle(1, 'one');
    expect(shown()).toEqual(['ready', ['one']]);
  });

  it('drops a failure an edit overtook', async () => {
    const { shown, settle, rerender, asked } = setup();
    rerender({ revision: {}, structure: 'p' });
    await act(async () => asked[0]!.reject(new Error('old')));
    expect(shown()).toEqual(['loading', []]);
    await settle(1, 'one');
    expect(shown()).toEqual(['ready', ['one']]);
  });

  it('asks for nothing more once the viewer is gone', async () => {
    const { settle, rerender, unmount, asked } = setup();
    rerender({ revision: {}, structure: 'p' });
    unmount();
    await settle(0, 'late');
    expect(asked).toHaveLength(1);
  });
});
