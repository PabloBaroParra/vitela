import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError, EDIT_NOT_PERMITTED, ENCODING_GAP } from '@vitela/core';
import type { Edit, EditedDocument, Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { MARKUP_COLOR, useAnnotationEdits } from './useAnnotationEdits';
import type { AddOutcome, MarkupOutcome } from './useAnnotationEdits';

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

/** What sits at position 0 of `info()`: page id 0, unrotated. */
const PAGE_0 = '0/0';

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

    let marked: MarkupOutcome = 'refused';
    await act(async () => {
      marked = await result.current.markup('highlight', 0, PAGE_0, LINES);
    });

    expect(marked).toBe('marked');
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

    let marked: MarkupOutcome = 'marked';
    await act(async () => {
      marked = await result.current.markup('underline', 0, PAGE_0, new Float32Array([...LINES, 72, 160, 100, 18]));
    });

    // Some lines went in: the caller must not offer the same lines again.
    expect(marked).toBe('partial');
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
    let marked: MarkupOutcome = 'marked';
    await act(async () => {
      marked = await result.current.markup('highlight', 0, PAGE_0, LINES);
    });
    expect(marked).toBe('refused');
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

    let first: Promise<MarkupOutcome> = Promise.resolve('refused');
    act(() => {
      first = result.current.markup('highlight', 0, PAGE_0, LINES.subarray(0, 4));
    });
    await waitFor(() => expect(result.current.pending).toBe(true));
    let second: MarkupOutcome = 'marked';
    await act(async () => {
      second = await result.current.markup('strikeout', 0, PAGE_0, LINES.subarray(0, 4));
    });
    expect(second).toBe('refused');
    expect(result.current.failure).toBeNull();
    expect(applyEdit).toHaveBeenCalledTimes(1);

    await act(async () => {
      release();
      await first;
    });
    expect(result.current.pending).toBe(false);
  });

  it('refuses a markup while an undo is still running', async () => {
    let release: () => void = () => {};
    const undo = vi.fn(
      () =>
        new Promise<EditedDocument>((resolve) => {
          release = () => resolve(edited(false, true));
        }),
    );
    const applyEdit = vi.fn(async () => edited(true));
    const engine = fakeEngine({ undo, applyEdit });
    const { result } = renderHook(() => useAnnotationEdits(engine, doc({ canUndo: true }), vi.fn()));

    let undoing: Promise<void> = Promise.resolve();
    act(() => {
      undoing = result.current.undo();
    });
    await waitFor(() => expect(result.current.pending).toBe(true));
    let marked: MarkupOutcome = 'marked';
    await act(async () => {
      marked = await result.current.markup('highlight', 0, PAGE_0, LINES);
    });
    expect(marked).toBe('refused');
    expect(result.current.failure).toBeNull();

    await act(async () => {
      release();
      await undoing;
    });
    expect(applyEdit).not.toHaveBeenCalled();
    expect(result.current.pending).toBe(false);
  });

  it('does not mark up a page that an edit put another page in place of', async () => {
    // The undo moved page id 1 to position 0; the viewer has not re-rendered yet.
    const moved = { ...info(), pageCount: 2, pages: [...info().pages, ...info().pages], pageIds: [1, 0] };
    const engine = fakeEngine({
      undo: async () => ({ ...edited(false, true), info: moved }),
      applyEdit: vi.fn(async () => edited(true)),
    });
    // One open document: its info stays what it was at open.
    const open = doc({ canUndo: true });
    const { result } = renderHook(() => useAnnotationEdits(engine, open, vi.fn()));

    await act(() => result.current.undo());
    let marked: MarkupOutcome = 'marked';
    await act(async () => {
      marked = await result.current.markup('highlight', 0, PAGE_0, LINES);
    });

    expect(marked).toBe('failed');
    expect(engine.applyEdit).not.toHaveBeenCalled();
    expect(result.current.failure).toMatch(/page changed/i);
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

    let marking: Promise<MarkupOutcome> = Promise.resolve('refused');
    act(() => {
      marking = result.current.markup('highlight', 0, PAGE_0, LINES.subarray(0, 4));
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

describe('useAnnotationEdits.add', () => {
  const ink: Edit = { kind: 'addInk', page: 0, points: [{ x: 1, y: 2 }, { x: 3, y: 4 }], color: MARKUP_COLOR };
  const freeText: Edit = { kind: 'addFreeText', page: 0, rect: { left: 1, top: 2, width: 200, height: 50 }, contents: '日' };

  it('adds one drawn annotation as one edit and reports it', async () => {
    const applyEdit = vi.fn(async () => edited(true));
    const onEdited = vi.fn();
    const { result } = renderHook(() => useAnnotationEdits(fakeEngine({ applyEdit }), doc(), onEdited));

    let outcome: AddOutcome = { status: 'refused' };
    await act(async () => {
      outcome = await result.current.add(ink, 0, PAGE_0);
    });

    expect(outcome).toEqual({ status: 'added' });
    expect(applyEdit).toHaveBeenCalledWith(7, ink);
    expect(onEdited).toHaveBeenCalledTimes(1);
  });

  it('returns an error the caller shows itself without making it the viewer failure', async () => {
    const engine = fakeEngine({
      applyEdit: () => Promise.reject(new EngineError(ENCODING_GAP, "'日' cannot be shown in Helvetica (WinAnsi)")),
    });
    const { result } = renderHook(() => useAnnotationEdits(engine, doc(), vi.fn()));

    let outcome: AddOutcome = { status: 'added' };
    await act(async () => {
      outcome = await result.current.add(freeText, 0, PAGE_0, [ENCODING_GAP]);
    });
    expect(outcome).toEqual({ status: 'failed', kind: ENCODING_GAP, message: "'日' cannot be shown in Helvetica (WinAnsi)" });
    expect(result.current.failure).toBeNull();

    // Not one the caller shows: the viewer shows it.
    await act(async () => {
      outcome = await result.current.add(freeText, 0, PAGE_0);
    });
    expect(result.current.failure).toMatch(/WinAnsi/);
  });

  it('refuses while a step runs, without permission, and on a page that changed under the gesture', async () => {
    const moved = { ...info(), pageCount: 2, pages: [...info().pages, ...info().pages], pageIds: [1, 0] };
    const applyEdit = vi.fn(async () => edited(true));
    const engine = fakeEngine({ applyEdit, undo: async () => ({ ...edited(false, true), info: moved }) });

    const denied = renderHook(() => useAnnotationEdits(engine, doc({ info: info(false) }), vi.fn()));
    await act(async () => {
      expect(await denied.result.current.add(ink, 0, PAGE_0)).toEqual({ status: 'refused' });
    });

    const { result } = renderHook(() => useAnnotationEdits(engine, doc({ canUndo: true }), vi.fn()));
    let undoing: Promise<void> = Promise.resolve();
    let during: AddOutcome = { status: 'added' };
    await act(async () => {
      undoing = result.current.undo();
      during = await result.current.add(ink, 0, PAGE_0);
      await undoing;
    });
    expect(during).toEqual({ status: 'refused' });

    let after: AddOutcome = { status: 'added' };
    await act(async () => {
      after = await result.current.add(ink, 0, PAGE_0);
    });
    expect(after).toMatchObject({ status: 'failed', kind: null });
    expect(result.current.failure).toMatch(/page changed/i);
    expect(applyEdit).not.toHaveBeenCalled();
  });
});
