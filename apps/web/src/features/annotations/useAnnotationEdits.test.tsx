import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError, EDIT_NOT_PERMITTED } from '@vitela/core';
import type { Edit, EditedDocument, Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { MARKUP_COLOR, useAnnotationEdits } from './useAnnotationEdits';

const info = (annotationEditingAllowed = true) => ({
  pageCount: 1,
  pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
  pageIds: [0],
  textExtractionAllowed: true,
  documentAssemblyAllowed: true,
  blocks: [],
  importRefusal: null,
  annotationEditingAllowed,
});

const doc = (overrides: Partial<OpenDocument> = {}): OpenDocument => ({
  name: 'a.pdf',
  info: info(),
  handle: 7,
  generation: 1,
  ...overrides,
});

const edited = (canUndo: boolean, canRedo = false): EditedDocument => ({
  info: info(),
  canUndo,
  canRedo,
  annotations: [],
  addedAnnotationId: null,
});

/** Two selected lines on page 0. */
const LINES = new Float32Array([72, 100, 200, 18, 72, 130, 150, 18]);

describe('useAnnotationEdits', () => {
  it('marks up each selected line as its own edit, in order, reporting each', async () => {
    const applied: Edit[] = [];
    const engine = fakeEngine({
      applyEdit: async (handle, edit) => {
        expect(handle).toBe(7);
        applied.push(edit);
        return edited(true);
      },
    });
    const onEdited = vi.fn();
    const { result } = renderHook(() => useAnnotationEdits(engine, doc(), onEdited));

    await act(() => result.current.markup('highlight', 0, LINES));

    expect(applied).toEqual([
      { kind: 'addMarkup', page: 0, markup: 'highlight', rect: { left: 72, top: 100, width: 200, height: 18 }, color: MARKUP_COLOR },
      { kind: 'addMarkup', page: 0, markup: 'highlight', rect: { left: 72, top: 130, width: 150, height: 18 }, color: MARKUP_COLOR },
    ]);
    expect(onEdited).toHaveBeenCalledTimes(2);
    expect(result.current.pending).toBe(false);
    expect(result.current.failure).toBeNull();
  });

  it('turns markup off when the core refuses it, keeping the lines already marked', async () => {
    let calls = 0;
    const engine = fakeEngine({
      applyEdit: async () => {
        if (++calls === 2) throw new EngineError(EDIT_NOT_PERMITTED, 'not permitted');
        return edited(true);
      },
    });
    const onEdited = vi.fn();
    const { result } = renderHook(() => useAnnotationEdits(engine, doc(), onEdited));

    await act(() => result.current.markup('underline', 0, new Float32Array([...LINES, 72, 160, 100, 18])));

    expect(calls).toBe(2);
    expect(onEdited).toHaveBeenCalledTimes(1);
    expect(result.current.allowed).toBe(false);
    expect(result.current.failure).toBe('not permitted');
  });

  it('does nothing when the document forbids annotating', async () => {
    const applyEdit = vi.fn();
    const engine = fakeEngine({ applyEdit });
    const { result } = renderHook(() => useAnnotationEdits(engine, doc({ info: info(false) }), vi.fn()));

    expect(result.current.allowed).toBe(false);
    await act(() => result.current.markup('highlight', 0, LINES));
    expect(applyEdit).not.toHaveBeenCalled();
  });

  it('ignores a second markup while one is running', async () => {
    let release: () => void = () => {};
    const applyEdit = vi.fn(
      () =>
        new Promise<EditedDocument>((resolve) => {
          release = () => resolve(edited(true));
        }),
    );
    const engine = fakeEngine({ applyEdit });
    const { result } = renderHook(() => useAnnotationEdits(engine, doc(), vi.fn()));

    let first: Promise<void> = Promise.resolve();
    act(() => {
      first = result.current.markup('highlight', 0, LINES.subarray(0, 4));
    });
    await waitFor(() => expect(result.current.pending).toBe(true));
    await act(() => result.current.markup('strikeout', 0, LINES.subarray(0, 4)));
    expect(applyEdit).toHaveBeenCalledTimes(1);

    await act(async () => {
      release();
      await first;
    });
    expect(result.current.pending).toBe(false);
  });

  it('undoes and redoes through the engine, after the step in flight', async () => {
    const order: string[] = [];
    const gate: { release: (() => void) | null } = { release: null };
    const engine: Engine = fakeEngine({
      applyEdit: () =>
        new Promise((resolve) => {
          gate.release = () => {
            order.push('edit');
            resolve(edited(true));
          };
        }),
      undo: async () => {
        order.push('undo');
        return edited(false, true);
      },
      redo: async () => {
        order.push('redo');
        return edited(true);
      },
    });
    const onEdited = vi.fn();
    const { result } = renderHook(() => useAnnotationEdits(engine, doc({ canUndo: true, canRedo: true }), onEdited));

    let marking: Promise<void> = Promise.resolve();
    act(() => {
      marking = result.current.markup('highlight', 0, LINES.subarray(0, 4));
    });
    let undoing: Promise<void> = Promise.resolve();
    act(() => {
      undoing = result.current.undo();
    });
    // The undo waits for the edit the engine is still working on.
    await waitFor(() => expect(gate.release).not.toBeNull());
    expect(order).toEqual([]);
    await act(async () => {
      gate.release!();
      await marking;
      await undoing;
    });
    await act(() => result.current.redo());

    expect(order).toEqual(['edit', 'undo', 'redo']);
    expect(onEdited).toHaveBeenCalledTimes(3);
  });

  it('asks nothing of the engine when there is nothing to undo or redo', async () => {
    const undo = vi.fn();
    const redo = vi.fn();
    const engine = fakeEngine({ undo, redo });
    const { result } = renderHook(() => useAnnotationEdits(engine, doc(), vi.fn()));

    expect(result.current.canUndo).toBe(false);
    await act(() => result.current.undo());
    await act(() => result.current.redo());
    expect(undo).not.toHaveBeenCalled();
    expect(redo).not.toHaveBeenCalled();
  });

  it('shows why an undo failed', async () => {
    const engine = fakeEngine({ undo: () => Promise.reject(new Error('worker gone')) });
    const { result } = renderHook(() => useAnnotationEdits(engine, doc({ canUndo: true }), vi.fn()));

    await act(() => result.current.undo());
    expect(result.current.failure).toBe('worker gone');
  });
});
