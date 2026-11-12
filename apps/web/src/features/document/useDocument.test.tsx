import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { EngineError } from '../../core/errors';
import type { DocumentInfo, Engine, OpenedDocument } from '../../core/types';
import { fakeEngine } from '../../test/fakeEngine';
import { useDocument } from './useDocument';

function file(name: string, bytes = [37, 80, 68, 70]): File {
  return new File([new Uint8Array(bytes)], name, { type: 'application/pdf' });
}

function info(pageCount: number): DocumentInfo {
  return {
    pageCount,
    pages: Array(pageCount).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }),
    textExtractionAllowed: true,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/**
 * An engine whose `open` is `open` with a handle added per successful open
 * (1, 2, ...), recording which handles the hook closes.
 */
function engineWith(open: (bytes: Uint8Array, password?: string) => Promise<DocumentInfo>) {
  let nextHandle = 1;
  const closed: number[] = [];
  const engine: Engine = fakeEngine({
    open: async (bytes, password): Promise<OpenedDocument> => {
      const opened = await open(bytes, password);
      return { handle: nextHandle++, info: opened };
    },
    close: async (handle) => {
      closed.push(handle);
    },
  });
  return Object.assign(engine, { closed });
}

/** An engine guarding its one document with `password`, like the core does. */
function lockedEngine(password: string, seen: (string | undefined)[] = []) {
  return engineWith(async (_bytes, given) => {
    seen.push(given);
    if (given === undefined) throw new EngineError('PasswordRequired', 'the document is encrypted');
    if (given !== password) throw new EngineError('WrongPassword', 'the password does not open it');
    return info(1);
  });
}

describe('useDocument', () => {
  it('starts with no document and nothing in progress', () => {
    const { result } = renderHook(() => useDocument(engineWith(() => new Promise(() => {}))));
    expect(result.current.state).toEqual({ document: null, activity: { kind: 'idle' } });
  });

  it('passes the file bytes to the engine and reports the opened document', async () => {
    const seen: number[][] = [];
    const engine = engineWith(async (bytes) => {
      seen.push([...bytes]);
      return info(2);
    });
    const { result } = renderHook(() => useDocument(engine));

    act(() => result.current.open(file('a.pdf', [1, 2, 3])));
    expect(result.current.state.activity).toEqual({ kind: 'opening', name: 'a.pdf' });

    await waitFor(() => expect(result.current.state.document).not.toBeNull());
    expect(result.current.state).toEqual({
      document: { name: 'a.pdf', info: info(2), handle: 1, generation: 1 },
      activity: { kind: 'idle' },
    });
    expect(seen).toEqual([[1, 2, 3]]);
  });

  it('reports a failed open with the engine message and keeps the open document', async () => {
    let fail = false;
    const engine = engineWith(async () => {
      if (fail) throw new EngineError('InvalidDocument', 'not a PDF');
      return info(1);
    });
    const { result } = renderHook(() => useDocument(engine));
    act(() => result.current.open(file('good.pdf')));
    await waitFor(() => expect(result.current.state.document?.name).toBe('good.pdf'));

    fail = true;
    act(() => result.current.open(file('broken.pdf')));
    await waitFor(() =>
      expect(result.current.state.activity).toEqual({ kind: 'failed', name: 'broken.pdf', message: 'not a PDF' }),
    );
    expect(result.current.state.document?.name).toBe('good.pdf');
  });

  it('asks for a password when the document needs one', async () => {
    const { result } = renderHook(() => useDocument(lockedEngine('secret')));
    act(() => result.current.open(file('locked.pdf')));
    await waitFor(() =>
      expect(result.current.state.activity).toEqual({ kind: 'password', name: 'locked.pdf', wrong: false, checking: false }),
    );
    expect(result.current.state.document).toBeNull();
  });

  it('re-reads the file for each password attempt and flags a wrong password', async () => {
    const seen: (string | undefined)[] = [];
    const { result } = renderHook(() => useDocument(lockedEngine('secret', seen)));
    act(() => result.current.open(file('locked.pdf')));
    await waitFor(() => expect(result.current.state.activity.kind).toBe('password'));

    act(() => result.current.submitPassword('guess'));
    expect(result.current.state.activity).toMatchObject({ kind: 'password', checking: true });
    await waitFor(() =>
      expect(result.current.state.activity).toEqual({ kind: 'password', name: 'locked.pdf', wrong: true, checking: false }),
    );

    act(() => result.current.submitPassword('secret'));
    await waitFor(() => expect(result.current.state.document?.name).toBe('locked.pdf'));
    expect(result.current.state.activity).toEqual({ kind: 'idle' });
    expect(seen).toEqual([undefined, 'guess', 'secret']);
  });

  it('cancelling the password prompt keeps the previously open document', async () => {
    let locked = false;
    const engine = engineWith(async (_bytes, password) => {
      if (locked && password === undefined) throw new EngineError('PasswordRequired', 'encrypted');
      return info(3);
    });
    const { result } = renderHook(() => useDocument(engine));
    act(() => result.current.open(file('first.pdf')));
    await waitFor(() => expect(result.current.state.document?.name).toBe('first.pdf'));

    locked = true;
    act(() => result.current.open(file('locked.pdf')));
    await waitFor(() => expect(result.current.state.activity.kind).toBe('password'));
    act(() => result.current.cancelPassword());

    expect(result.current.state).toEqual({
      document: { name: 'first.pdf', info: info(3), handle: 1, generation: 1 },
      activity: { kind: 'idle' },
    });
  });

  it('shows a non-password failure during a password attempt as a failed open', async () => {
    const engine = engineWith(async (_bytes, password) => {
      if (password === undefined) throw new EngineError('PasswordRequired', 'encrypted');
      throw new EngineError('UnsupportedSecurityHandler', 'cannot decrypt this handler');
    });
    const { result } = renderHook(() => useDocument(engine));
    act(() => result.current.open(file('odd.pdf')));
    await waitFor(() => expect(result.current.state.activity.kind).toBe('password'));
    act(() => result.current.submitPassword('x'));
    await waitFor(() =>
      expect(result.current.state.activity).toEqual({
        kind: 'failed',
        name: 'odd.pdf',
        message: 'cannot decrypt this handler',
      }),
    );
  });

  it('ignores a slower earlier open once a later one was started', async () => {
    const first = deferred<DocumentInfo>();
    const second = deferred<DocumentInfo>();
    const pending = [first, second];
    const engine = engineWith(() => pending.shift()!.promise);
    const { result } = renderHook(() => useDocument(engine));

    act(() => result.current.open(file('first.pdf')));
    act(() => result.current.open(file('second.pdf')));
    await act(async () => second.resolve(info(5)));
    await act(async () => first.resolve(info(1)));

    expect(result.current.state.document).toMatchObject({ name: 'second.pdf', info: info(5) });
  });

  it('ignores a password submitted after a newer file was picked', async () => {
    const seen: (string | undefined)[] = [];
    const { result } = renderHook(() => useDocument(lockedEngine('secret', seen)));
    act(() => result.current.open(file('locked.pdf')));
    await waitFor(() => expect(result.current.state.activity.kind).toBe('password'));
    act(() => result.current.cancelPassword());
    act(() => result.current.submitPassword('secret'));
    expect(result.current.state.activity).toEqual({ kind: 'idle' });
    expect(seen).toEqual([undefined]);
  });

  it('closes the document it replaces, once the new one opened', async () => {
    const engine = engineWith(async () => info(1));
    const { result } = renderHook(() => useDocument(engine));
    act(() => result.current.open(file('first.pdf')));
    await waitFor(() => expect(result.current.state.document?.handle).toBe(1));
    expect(engine.closed).toEqual([]);

    act(() => result.current.open(file('second.pdf')));
    await waitFor(() => expect(result.current.state.document?.handle).toBe(2));
    expect(engine.closed).toEqual([1]);
  });

  it('closes a document whose password check finished after Cancel, and keeps the shown one', async () => {
    const check = deferred<DocumentInfo>();
    let locked = false;
    const engine = engineWith(async (_bytes, password) => {
      if (!locked) return info(3);
      if (password === undefined) throw new EngineError('PasswordRequired', 'encrypted');
      return check.promise;
    });
    const { result } = renderHook(() => useDocument(engine));
    act(() => result.current.open(file('first.pdf')));
    await waitFor(() => expect(result.current.state.document?.handle).toBe(1));

    locked = true;
    act(() => result.current.open(file('locked.pdf')));
    await waitFor(() => expect(result.current.state.activity.kind).toBe('password'));
    act(() => result.current.submitPassword('secret'));
    act(() => result.current.cancelPassword());
    // The engine opened it anyway: the UI must hand it back, not ignore it.
    await act(async () => check.resolve(info(1)));

    expect(result.current.state.document).toMatchObject({ name: 'first.pdf', handle: 1 });
    expect(engine.closed).toEqual([2]);
  });

  it('closes a slower earlier open that a later one superseded', async () => {
    const first = deferred<DocumentInfo>();
    const second = deferred<DocumentInfo>();
    const pending = [first, second];
    const engine = engineWith(() => pending.shift()!.promise);
    const { result } = renderHook(() => useDocument(engine));

    act(() => result.current.open(file('first.pdf')));
    act(() => result.current.open(file('second.pdf')));
    await act(async () => second.resolve(info(5)));
    await act(async () => first.resolve(info(1)));

    // second.pdf resolved first and got handle 1; the late first.pdf got 2.
    expect(result.current.state.document).toMatchObject({ name: 'second.pdf', handle: 1 });
    expect(engine.closed).toEqual([2]);
  });
});
