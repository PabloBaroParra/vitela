import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useDraftEditor, type Drafted } from './useDraftEditor';

afterEach(() => vi.useRealTimers());

interface Target {
  readonly name: string;
  readonly size?: number;
}

function setup() {
  const commits: Drafted<Target>[] = [];
  const focusPages = vi.fn();
  const hook = renderHook(({ active }: { active: boolean }) => useDraftEditor<Target>({ active, onCommit: (editor) => void commits.push(editor), focusPages }), {
    initialProps: { active: true },
  });
  return { hook, commits, focusPages };
}

describe('useDraftEditor', () => {
  it('holds what is typed and commits it once, closing a turn later', () => {
    vi.useFakeTimers();
    const { hook, commits, focusPages } = setup();
    act(() => void hook.result.current.open({ name: 'a' }, 'start'));
    act(() => hook.result.current.typing('typed'));
    expect(hook.result.current.editor).toEqual({ name: 'a', draft: 'typed' });

    act(() => {
      hook.result.current.commit(true);
      // The blur the closing causes asks again: only the first counts.
      hook.result.current.commit(false);
    });
    expect(commits).toEqual([{ name: 'a', draft: 'typed' }]);
    expect(focusPages).toHaveBeenCalledTimes(1);
    expect(hook.result.current.editor).not.toBeNull();
    act(() => void vi.runAllTimers());
    expect(hook.result.current.editor).toBeNull();
  });

  it('cancels without recording, and a commit after it counts for nothing', () => {
    vi.useFakeTimers();
    const { hook, commits } = setup();
    act(() => void hook.result.current.open({ name: 'a' }, ''));
    act(() => {
      hook.result.current.cancel();
      hook.result.current.commit(false);
    });
    act(() => void vi.runAllTimers());
    expect(commits).toEqual([]);
    expect(hook.result.current.editor).toBeNull();
  });

  it('commits the draft when the field goes, unless it is mounted again at once', () => {
    vi.useFakeTimers();
    const { hook, commits } = setup();
    act(() => void hook.result.current.open({ name: 'a' }, 'kept'));
    act(() => {
      hook.result.current.detach();
      hook.result.current.attach();
    });
    act(() => void vi.runAllTimers());
    expect(commits).toEqual([]);

    act(() => hook.result.current.detach());
    expect(commits).toEqual([]);
    act(() => void vi.runAllTimers());
    expect(commits).toEqual([{ name: 'a', draft: 'kept' }]);
  });

  it('commits the open editor when the mode is turned off', () => {
    const { hook, commits } = setup();
    act(() => void hook.result.current.open({ name: 'a' }, 'left'));
    hook.rerender({ active: false });
    expect(commits).toEqual([{ name: 'a', draft: 'left' }]);
  });

  it('patches only the editor it was opened as', () => {
    vi.useFakeTimers();
    const { hook } = setup();
    let first = 0;
    act(() => void (first = hook.result.current.open({ name: 'a' }, '')));
    act(() => hook.result.current.patch(first, { size: 12 }));
    expect(hook.result.current.editor).toMatchObject({ size: 12 });
    act(() => hook.result.current.commit(false));
    act(() => void vi.runAllTimers());
    act(() => void hook.result.current.open({ name: 'b' }, ''));
    act(() => hook.result.current.patch(first, { size: 99 }));
    expect(hook.result.current.editor).toEqual({ name: 'b', draft: '' });
  });
});
