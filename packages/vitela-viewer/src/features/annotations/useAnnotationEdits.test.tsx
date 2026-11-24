import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EngineError, EDIT_NOT_PERMITTED, ENCODING_GAP } from '@vitela/core';
import type { Annotation, AnnotationEdit, Edit, EditedDocument, Engine, PageSize } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { MARKUP_COLOR, useAnnotationEdits } from './useAnnotationEdits';
import type { AddOutcome, ChangeOutcome, MarkupOutcome } from './useAnnotationEdits';

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

  it('hands the added annotation id over before the edited document is reported', async () => {
    const applyEdit = vi.fn(async (): Promise<EditedDocument> => ({ ...edited(true), addedAnnotationId: 42 }));
    const order: string[] = [];
    const onEdited = vi.fn(() => order.push('edited'));
    const onAdded = vi.fn((id: number) => order.push(`added ${id}`));
    const { result } = renderHook(() => useAnnotationEdits(fakeEngine({ applyEdit }), doc(), onEdited));

    await act(async () => {
      await result.current.add(ink, 0, PAGE_0, [], onAdded);
    });
    // In the same step, so the layer never draws the annotation without what the caller keeps for it.
    expect(order).toEqual(['added 42', 'edited']);

    // An answer with no added id calls nothing.
    applyEdit.mockResolvedValueOnce(edited(true));
    await act(async () => {
      await result.current.add(ink, 0, PAGE_0, [], onAdded);
    });
    expect(onAdded).toHaveBeenCalledTimes(1);
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

  it('builds a deferred edit inside its own step, so a second add meanwhile is refused before it builds anything', async () => {
    const applyEdit = vi.fn(async () => edited(true));
    const { result } = renderHook(() => useAnnotationEdits(fakeEngine({ applyEdit }), doc(), vi.fn()));
    const inkEdit = ink as AnnotationEdit;
    let finishBuild: (edit: AnnotationEdit) => void = () => {};
    const build = vi.fn(() => new Promise<AnnotationEdit>((resolve) => (finishBuild = resolve)));
    const second = vi.fn(async () => inkEdit);

    let first: Promise<AddOutcome> = Promise.resolve({ status: 'refused' });
    let during: AddOutcome = { status: 'added' };
    await act(async () => {
      first = result.current.add(build, 0, PAGE_0);
      // The build is part of the step: the queue is busy from the call on.
      during = await result.current.add(second, 0, PAGE_0);
    });
    expect(during).toEqual({ status: 'refused' });
    expect(second).not.toHaveBeenCalled();
    await waitFor(() => expect(build).toHaveBeenCalledTimes(1));

    await act(async () => {
      finishBuild(inkEdit);
      expect(await first).toEqual({ status: 'added' });
    });
    expect(applyEdit).toHaveBeenCalledWith(7, ink);
  });

  it('checks the page before building a deferred edit, and shows why a build failed', async () => {
    const applyEdit = vi.fn(async () => edited(true));
    const { result } = renderHook(() => useAnnotationEdits(fakeEngine({ applyEdit }), doc(), vi.fn()));
    const build = vi.fn(async () => ink as AnnotationEdit);
    let outcome: AddOutcome = { status: 'added' };
    await act(async () => {
      outcome = await result.current.add(build, 0, '9/0');
    });
    expect(outcome).toMatchObject({ status: 'failed', kind: null });
    expect(build).not.toHaveBeenCalled();

    await act(async () => {
      outcome = await result.current.add(() => Promise.reject(new Error('The image could not be decoded')), 0, PAGE_0);
    });
    expect(outcome).toMatchObject({ status: 'failed', message: 'The image could not be decoded' });
    expect(result.current.failure).toBe('The image could not be decoded');
    expect(applyEdit).not.toHaveBeenCalled();
  });

  it('shows a notice a caller raises until the next step starts', async () => {
    const engine = fakeEngine({ applyEdit: vi.fn(async () => edited(true)) });
    const { result } = renderHook(() => useAnnotationEdits(engine, doc(), vi.fn()));
    act(() => result.current.notify('Nothing was added.'));
    expect(result.current.failure).toBe('Nothing was added.');
    await act(async () => {
      await result.current.add(ink, 0, PAGE_0);
    });
    expect(result.current.failure).toBeNull();
  });
});

describe('useAnnotationEdits.change', () => {
  const rect = { left: 10, top: 20, width: 30, height: 40 };
  const shapeAt = (left: number): Annotation => ({ id: 4, pageIndex: 0, kind: 'shape', rect: { ...rect, left }, color: MARKUP_COLOR });
  const withShape = (left: number): EditedDocument => ({ ...edited(true), annotations: [shapeAt(left)] });
  const nudge = (current: Annotation): AnnotationEdit => ({ kind: 'moveAnnotation', id: current.id, dx: 5, dy: 0 });
  /** An engine that moves the shape, and what it was asked. */
  function movingEngine() {
    const applied: Edit[] = [];
    let left = 10;
    const applyEdit = vi.fn(async (_handle: number, edit: Edit) => {
      applied.push(edit);
      if (edit.kind === 'moveAnnotation') left += edit.dx;
      return withShape(left);
    });
    return { engine: fakeEngine({ applyEdit }), applyEdit, applied };
  }
  const session = () => doc({ annotations: [shapeAt(10)] });

  it('builds each change from the annotation as the last step left it, queuing behind a running one', async () => {
    const { engine, applied } = movingEngine();
    const onEdited = vi.fn();
    const { result } = renderHook(() => useAnnotationEdits(engine, session(), onEdited));
    const seen: number[] = [];
    const build = (current: Annotation, page: PageSize) => {
      expect(page.widthPt).toBe(612);
      seen.push((current as { rect: { left: number } }).rect.left);
      return nudge(current);
    };
    let outcomes: ChangeOutcome[] = [];
    await act(async () => {
      outcomes = await Promise.all([result.current.change(4, PAGE_0, build), result.current.change(4, PAGE_0, build)]);
    });
    expect(outcomes).toEqual([{ status: 'changed' }, { status: 'changed' }]);
    expect(seen).toEqual([10, 15]);
    expect(applied).toHaveLength(2);
    expect(onEdited).toHaveBeenCalledTimes(2);
  });

  it('records nothing when the build asks for nothing', async () => {
    const { engine, applyEdit } = movingEngine();
    const onEdited = vi.fn();
    const { result } = renderHook(() => useAnnotationEdits(engine, session(), onEdited));
    let outcome: ChangeOutcome = { status: 'refused' };
    await act(async () => {
      outcome = await result.current.change(4, PAGE_0, () => null);
    });
    expect(outcome).toEqual({ status: 'changed' });
    expect(applyEdit).not.toHaveBeenCalled();
    expect(onEdited).not.toHaveBeenCalled();
  });

  it('fails, saying so, when the annotation is gone by the time its step runs', async () => {
    const { engine, applyEdit } = movingEngine();
    const { result } = renderHook(() => useAnnotationEdits(engine, doc({ annotations: [] }), vi.fn()));
    let outcome: ChangeOutcome = { status: 'changed' };
    await act(async () => {
      outcome = await result.current.change(4, PAGE_0, nudge);
    });
    expect(outcome).toMatchObject({ status: 'failed', kind: null });
    expect(result.current.failure).toMatch(/no longer there/);
    expect(applyEdit).not.toHaveBeenCalled();
  });

  it('does not apply a drag to an annotation that moved since it was grabbed, and says so', async () => {
    const { engine, applied } = movingEngine();
    const { result } = renderHook(() => useAnnotationEdits(engine, session(), vi.fn()));
    const grabbed = shapeAt(10);
    let drag: ChangeOutcome = { status: 'changed' };
    await act(async () => {
      const first = result.current.change(4, PAGE_0, nudge);
      drag = await result.current.change(4, PAGE_0, nudge, { grabbed });
      await first;
    });
    expect(applied).toHaveLength(1);
    expect(drag).toMatchObject({ status: 'failed', kind: null });
    expect(result.current.failure).toMatch(/changed while it was being dragged/);
  });

  it('applies a drag to an annotation still where it was grabbed', async () => {
    const { engine, applied } = movingEngine();
    const { result } = renderHook(() => useAnnotationEdits(engine, session(), vi.fn()));
    await act(async () => {
      expect(await result.current.change(4, PAGE_0, nudge, { grabbed: shapeAt(10) })).toEqual({ status: 'changed' });
    });
    expect(applied).toHaveLength(1);
  });

  it('changes nothing when its page was turned meanwhile', async () => {
    const { engine, applyEdit } = movingEngine();
    const { result } = renderHook(() => useAnnotationEdits(engine, session(), vi.fn()));
    await act(async () => {
      await result.current.change(4, '0/90', nudge);
    });
    expect(applyEdit).not.toHaveBeenCalled();
    expect(result.current.failure).toMatch(/page changed/);
  });

  it('drops a change still waiting when the viewer goes away, and reports the one under way', async () => {
    let answer: (edited: EditedDocument) => void = () => {};
    const applyEdit = vi.fn(() => new Promise<EditedDocument>((resolve) => (answer = resolve)));
    const onEdited = vi.fn();
    const { result, unmount } = renderHook(() => useAnnotationEdits(fakeEngine({ applyEdit }), session(), onEdited), {
      reactStrictMode: true,
    });
    const changes = [result.current.change(4, PAGE_0, nudge), result.current.change(4, PAGE_0, nudge)];
    await waitFor(() => expect(applyEdit).toHaveBeenCalledTimes(1));
    unmount();
    answer(withShape(15));
    await Promise.all(changes);
    expect(applyEdit).toHaveBeenCalledTimes(1);
    expect(onEdited).toHaveBeenCalledTimes(1);
  });

  it('is refused without permission, and leaves an error the caller shows to it', async () => {
    const denied = renderHook(() => useAnnotationEdits(fakeEngine(), doc({ info: info(false), annotations: [shapeAt(10)] }), vi.fn()));
    await act(async () => {
      expect(await denied.result.current.change(4, PAGE_0, nudge)).toEqual({ status: 'refused' });
    });

    const engine = fakeEngine({ applyEdit: () => Promise.reject(new EngineError(ENCODING_GAP, 'cannot be shown')) });
    const { result } = renderHook(() => useAnnotationEdits(engine, session(), vi.fn()));
    let outcome: ChangeOutcome = { status: 'changed' };
    await act(async () => {
      outcome = await result.current.change(4, PAGE_0, nudge, { shownByCaller: [ENCODING_GAP] });
    });
    expect(outcome).toEqual({ status: 'failed', kind: ENCODING_GAP, message: 'cannot be shown' });
    expect(result.current.failure).toBeNull();
  });
});
