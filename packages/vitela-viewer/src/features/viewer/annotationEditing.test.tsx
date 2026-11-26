// Selecting and editing a session annotation in the viewer, end to end in the
// UI: a small in-memory engine stands in for the core (it applies each edit
// and answers with the snapshot), so every test reads both what was asked of
// it and what the viewer then shows.

import { act, cleanup, createEvent, fireEvent, render, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { EngineError, ENCODING_GAP } from '@vitela/core';
import type { Annotation, AnnotationEdit, Edit, EditedDocument, Engine, PageSize, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { previewEdit } from '../annotations/editGeometry';
import type { OpenDocument } from '../document/useDocument';
import { MarkDirtyContext } from '../save/MarkDirtyContext';
import { DocumentViewer } from './DocumentViewer';

const VIEWPORT = { width: 1000, height: 600 };
const bitmap: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };
const pages: PageSize[] = Array(3).fill({ widthPt: 612, heightPt: 792, rotationDegrees: 0 });
const RED = { r: 220, g: 40, b: 40 };
const box = { left: 100, top: 200, width: 80, height: 40 };

const shape: Annotation = { id: 3, pageIndex: 0, kind: 'shape', rect: box, color: RED };
const stamp: Annotation = { id: 6, pageIndex: 0, kind: 'stamp', rect: box };
const ink: Annotation = {
  id: 4,
  pageIndex: 0,
  kind: 'ink',
  points: [
    { x: 10, y: 20 },
    { x: 30, y: 40 },
  ],
  color: RED,
};
const textBox: Annotation = {
  id: 7,
  pageIndex: 0,
  kind: 'freeText',
  rect: box,
  contents: 'Total',
  layout: { fontSizePt: 12, lines: [], overflow: false },
};

function openDocument(annotations: readonly Annotation[], annotationEditingAllowed = true): OpenDocument {
  return {
    name: 'a.pdf',
    info: {
      pageCount: pages.length,
      pages,
      pageIds: pages.map((_, index) => index),
      textExtractionAllowed: true,
      documentAssemblyAllowed: true,
      blocks: [],
      importRefusal: null,
      annotationEditingAllowed,
      formFillAllowed: true,
    },
    handle: 1,
    generation: 1,
    annotations,
    canUndo: annotations.length > 0,
    canRedo: false,
  };
}

/** The session's edits applied in memory, as the core would. */
function session(initial: readonly Annotation[], overrides: Partial<Engine> = {}) {
  let history: (readonly Annotation[])[] = [initial];
  const edits: AnnotationEdit[] = [];
  const info = openDocument(initial).info;
  const answer = (): EditedDocument => ({
    info,
    canUndo: history.length > 1,
    canRedo: false,
    annotations: history[history.length - 1]!,
    addedAnnotationId: null, formFields: [],
  });
  const changed = (annotation: Annotation, edit: AnnotationEdit): Annotation[] => {
    if (!('id' in edit) || edit.id !== annotation.id) return [annotation];
    if (edit.kind === 'removeAnnotation') return [];
    if (edit.kind === 'recolorAnnotation') return [{ ...annotation, color: edit.color } as Annotation];
    if (edit.kind === 'retextAnnotation') return [{ ...annotation, contents: edit.contents } as Annotation];
    return [previewEdit(annotation, edit)];
  };
  const engine = fakeEngine({
    renderPage: async () => bitmap,
    applyEdit: async (_handle: number, edit: Edit) => {
      const annotationEdit = edit as AnnotationEdit;
      edits.push(annotationEdit);
      history = [...history, history[history.length - 1]!.flatMap((annotation) => changed(annotation, annotationEdit))];
      return answer();
    },
    undo: async () => {
      if (history.length > 1) history = history.slice(0, -1);
      else history = [[]];
      return answer();
    },
    freeTextLayout: async () => ({ fontSizePt: 12, lines: [], overflow: false }),
    ...overrides,
  });
  return { engine, edits };
}

function Host({ engine, initial }: { engine: Engine; initial: OpenDocument }) {
  const [document, setDocument] = useState(initial);
  return (
    <MarkDirtyContext.Provider value={() => {}}>
      <DocumentViewer
        engine={engine}
        document={document}
        onEdited={(_handle, edited) => {
          setDocument((shown) => ({ ...shown, ...edited }));
          return true;
        }}
        onStampImage={() => {}}
      />
    </MarkDirtyContext.Provider>
  );
}

function open(annotations: readonly Annotation[], overrides: Partial<Engine> = {}, allowed = true) {
  const { engine, edits } = session(annotations, overrides);
  const view = render(<Host engine={engine} initial={openDocument(annotations, allowed)} />);
  const target = (name: string) => view.getByRole('button', { name });
  const toolbar = () => view.queryByRole('toolbar', { name: 'Annotation' });
  const inToolbar = (name: string) => within(toolbar()!).getByRole('button', { name });
  const textLayer = () => view.container.querySelector('[data-page="1"] .text-layer')!;
  return { ...view, edits, target, toolbar, inToolbar, textLayer };
}

/** CSS px per point at 100%; nothing is laid out, so pointer offsets are × points. */
const PX = 96 / 72;
const at = (x: number, y: number, pointerId = 1) => ({
  pointerId,
  button: 0,
  buttons: 1,
  isPrimary: true,
  clientX: x * PX,
  clientY: y * PX,
});

function press(element: Element, x = 110, y = 210) {
  fireEvent.pointerDown(element, at(x, y));
  fireEvent.pointerUp(element, at(x, y));
}

/** Drags `element` by (`dx`, `dy`) points. */
async function drag(element: Element, dx: number, dy: number) {
  fireEvent.pointerDown(element, at(110, 210));
  fireEvent.pointerMove(element, at(110 + dx / 2, 210 + dy / 2));
  fireEvent.pointerMove(element, at(110 + dx, 210 + dy));
  await act(async () => {
    fireEvent.pointerUp(element, at(110 + dx, 210 + dy));
  });
}

async function key(element: Element, init: { key: string; shiftKey?: boolean; altKey?: boolean; repeat?: boolean }) {
  await act(async () => {
    fireEvent.keyDown(element, init);
  });
}

const shapeRect = (container: HTMLElement) => container.querySelector('.annotation-layer [data-annotation-id="3"]')!;
const rounded = (edit: AnnotationEdit | undefined) => {
  if (edit?.kind === 'moveAnnotation') return { ...edit, dx: Math.round(edit.dx), dy: Math.round(edit.dy) };
  if (edit?.kind === 'resizeAnnotation') {
    const { left, top, width, height } = edit.rect;
    const [l, t, w, h] = [left, top, width, height].map((length) => Math.round(length)) as [number, number, number, number];
    return { ...edit, rect: { left: l, top: t, width: w, height: h } };
  }
  return edit;
};

beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(VIEWPORT.width);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(VIEWPORT.height);
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    putImageData: () => {},
  } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
});
afterEach(cleanup);

describe('selecting an annotation', () => {
  it('selects the pressed annotation, with its toolbar and handles, and starts no text selection', () => {
    const selectText = vi.fn();
    const { target, toolbar, getAllByRole } = open([shape], { selectText });
    expect(toolbar()).toBeNull();
    press(target('Shape on page 1'));
    expect(target('Shape on page 1').getAttribute('aria-pressed')).toBe('true');
    expect(document.activeElement).toBe(target('Shape on page 1'));
    expect(toolbar()?.textContent).toContain('Shape');
    expect(getAllByRole('button', { name: /^Resize from the / })).toHaveLength(4);
    expect(selectText).not.toHaveBeenCalled();
  });

  it('lets go on a press elsewhere, which still selects text', async () => {
    const selectText = vi.fn(async () => ({ text: 'words', rects: new Float32Array([1, 2, 3, 4]) }));
    const { target, toolbar, textLayer, container } = open([shape], { selectText });
    press(target('Shape on page 1'));
    fireEvent.pointerDown(textLayer(), at(300, 300));
    fireEvent.pointerMove(textLayer(), at(340, 300));
    expect(toolbar()).toBeNull();
    expect(target('Shape on page 1').getAttribute('aria-pressed')).toBe('false');
    await waitFor(() => expect(container.querySelectorAll('.text-selection')).toHaveLength(1));

    // And the other way round: selecting an annotation drops the text selection.
    fireEvent.pointerUp(textLayer(), at(340, 300));
    press(target('Shape on page 1'));
    expect(container.querySelectorAll('.text-selection')).toHaveLength(0);
  });

  it('keeps it selected through a press on its toolbar, and a second finger', () => {
    const { target, toolbar, inToolbar, textLayer } = open([shape]);
    press(target('Shape on page 1'));
    press(inToolbar('Blue'));
    fireEvent.pointerDown(textLayer(), { ...at(300, 300, 2), isPrimary: false });
    expect(toolbar()).not.toBeNull();
  });

  it('selects the focused annotation with Enter and lets go with Escape, the focus staying on it', async () => {
    const { target, toolbar, getByRole } = open([shape]);
    target('Shape on page 1').focus();
    expect(toolbar()).toBeNull();
    await key(target('Shape on page 1'), { key: 'Enter' });
    expect(toolbar()).not.toBeNull();
    // Escape on a handle too: the handle goes, the focus goes to the annotation.
    const handle = getByRole('button', { name: 'Resize from the top left' });
    handle.focus();
    await key(handle, { key: 'Escape' });
    expect(toolbar()).toBeNull();
    expect(document.activeElement).toBe(target('Shape on page 1'));
  });

  it('selects on a click made with no pointer (assistive technology)', () => {
    const { target, toolbar } = open([shape]);
    fireEvent.click(target('Shape on page 1'), { detail: 1 });
    expect(toolbar()).toBeNull();
    fireEvent.click(target('Shape on page 1'), { detail: 0 });
    expect(toolbar()).not.toBeNull();
  });

  it('offers nothing while a drawing tool is armed', () => {
    const { target, toolbar, getByRole, queryByRole } = open([shape]);
    press(target('Shape on page 1'));
    fireEvent.click(getByRole('button', { name: 'Ink' }));
    expect(toolbar()).toBeNull();
    expect(queryByRole('button', { name: 'Shape on page 1' })).toBeNull();
    fireEvent.click(getByRole('button', { name: 'Select' }));
    expect(toolbar()).toBeNull();
    expect(target('Shape on page 1')).toBeTruthy();
  });

  it('offers nothing in a document that forbids annotations', () => {
    const { queryByRole } = open([shape], {}, false);
    expect(queryByRole('button', { name: 'Shape on page 1' })).toBeNull();
  });

  it('lets go when an undo takes the annotation away, and moves the focus to the pages', async () => {
    const { target, toolbar, getByRole, queryByRole, getByLabelText } = open([shape]);
    press(target('Shape on page 1'));
    await act(async () => {
      // From the keyboard, so the focus is on the annotation that goes.
      fireEvent.keyDown(target('Shape on page 1'), { key: 'z', ctrlKey: true });
    });
    expect(queryByRole('button', { name: 'Shape on page 1' })).toBeNull();
    expect(toolbar()).toBeNull();
    expect(document.activeElement).toBe(getByLabelText('Pages'));
    expect(getByRole('button', { name: 'Undo' }).getAttribute('aria-disabled')).toBe('true');
  });
});

describe('dragging', () => {
  it('moves the annotation as one edit, showing it on the way', async () => {
    const { target, edits, container } = open([shape]);
    const body = target('Shape on page 1');
    fireEvent.pointerDown(body, at(110, 210));
    fireEvent.pointerMove(body, at(130, 215));
    expect(Math.round(Number(shapeRect(container).getAttribute('x')))).toBe(120);
    fireEvent.pointerMove(body, at(150, 220));
    expect(edits).toHaveLength(0);
    await act(async () => {
      fireEvent.pointerUp(body, at(150, 220));
    });
    expect(edits.map(rounded)).toEqual([{ kind: 'moveAnnotation', id: 3, dx: 40, dy: 10 }]);
    expect(Math.round(Number(shapeRect(container).getAttribute('x')))).toBe(140);
  });

  it('records nothing for a press that does not travel', async () => {
    const { target, edits } = open([shape]);
    await drag(target('Shape on page 1'), 0, 0);
    await drag(target('Shape on page 1'), 1, 1);
    expect(edits).toEqual([]);
  });

  it('records nothing for a drag that comes back to where it began', async () => {
    const { target, edits } = open([shape]);
    const body = target('Shape on page 1');
    fireEvent.pointerDown(body, at(110, 210));
    fireEvent.pointerMove(body, at(200, 210));
    await act(async () => {
      fireEvent.pointerUp(body, at(110, 210));
    });
    expect(edits).toEqual([]);
  });

  it('stops at the page edge', async () => {
    const { target, edits } = open([shape]);
    await drag(target('Shape on page 1'), -400, 0);
    expect(edits.map(rounded)).toEqual([{ kind: 'moveAnnotation', id: 3, dx: -100, dy: 0 }]);
  });

  it('is dropped by Escape, a cancelled pointer or a lost capture', async () => {
    const { target, edits, container, toolbar } = open([shape]);
    const body = target('Shape on page 1');
    const drops = [
      () => fireEvent.keyDown(body, { key: 'Escape' }),
      () => fireEvent.pointerCancel(body, at(150, 220)),
      () => fireEvent.lostPointerCapture(body, at(150, 220)),
    ];
    for (const drop of drops) {
      fireEvent.pointerDown(body, at(110, 210));
      fireEvent.pointerMove(body, at(150, 220));
      drop();
      expect(shapeRect(container).getAttribute('x')).toBe('100');
      await act(async () => {
        fireEvent.pointerUp(body, at(150, 220));
      });
    }
    expect(edits).toEqual([]);
    // Escape gave up the drag, not the selection.
    expect(toolbar()).not.toBeNull();
  });

  it('is dropped when what was pressed goes away, and does not come back with it', () => {
    const { target, getByRole, container } = open([shape]);
    fireEvent.pointerDown(target('Shape on page 1'), at(110, 210));
    fireEvent.pointerMove(target('Shape on page 1'), at(150, 220));
    fireEvent.click(getByRole('button', { name: 'Ink' }));
    expect(shapeRect(container).getAttribute('x')).toBe('100');
    fireEvent.click(getByRole('button', { name: 'Select' }));
    fireEvent.pointerMove(target('Shape on page 1'), at(160, 220));
    expect(shapeRect(container).getAttribute('x')).toBe('100');
    press(target('Shape on page 1'));
    const handle = getByRole('button', { name: 'Resize from the bottom right' });
    fireEvent.pointerDown(handle, at(180, 240));
    fireEvent.pointerMove(handle, at(220, 260));
    fireEvent.keyDown(target('Shape on page 1'), { key: 'Enter' });
    expect(shapeRect(container).getAttribute('width')).toBe('80');
  });

  it('is dropped once the primary button is up, whoever else is down', async () => {
    const { target, edits, container } = open([shape]);
    const body = target('Shape on page 1');
    fireEvent.pointerDown(body, at(110, 210));
    fireEvent.pointerMove(body, at(150, 220));
    fireEvent.pointerMove(body, { ...at(160, 220), buttons: 2 });
    expect(shapeRect(container).getAttribute('x')).toBe('100');
    fireEvent.pointerDown(body, at(110, 210));
    fireEvent.pointerMove(body, at(150, 220));
    await act(async () => {
      fireEvent.pointerUp(body, { ...at(150, 220), button: 2, buttons: 0 });
    });
    expect(edits).toEqual([]);
  });

  it('can be dragged again before the first drag has landed', async () => {
    const answers: (() => void)[] = [];
    const base = session([shape]);
    const { target, container, queryByRole } = open([shape], {
      applyEdit: async (handle, edit) => {
        await new Promise<void>((resolve) => answers.push(resolve));
        return base.engine.applyEdit(handle, edit);
      },
    });
    await drag(target('Shape on page 1'), 40, 0);
    fireEvent.pointerDown(target('Shape on page 1'), at(110, 210));
    expect(Math.round(Number(shapeRect(container).getAttribute('x')))).toBe(140);
    await drag(target('Shape on page 1'), 10, 5);
    expect(Math.round(Number(shapeRect(container).getAttribute('x')))).toBe(150);
    for (let i = 0; i < 2; i++) {
      await waitFor(() => expect(answers).toHaveLength(i + 1));
      await act(async () => answers[i]!());
    }
    expect(base.edits).toHaveLength(2);
    expect(queryByRole('alert')).toBeNull();
  });

  it('says so, and changes nothing, when the annotation moved under the drag', async () => {
    const { target, edits, findByRole } = open([shape]);
    const body = target('Shape on page 1');
    fireEvent.pointerDown(body, at(110, 210));
    await key(body, { key: 'ArrowRight' });
    fireEvent.pointerMove(body, at(150, 220));
    await act(async () => {
      fireEvent.pointerUp(body, at(150, 220));
    });
    expect(edits).toHaveLength(1);
    expect((await findByRole('alert')).textContent).toMatch(/changed while it was being dragged/);
  });

  it('resizes by a corner, the opposite one staying', async () => {
    const { target, edits, getByRole } = open([shape]);
    press(target('Shape on page 1'));
    await drag(getByRole('button', { name: 'Resize from the bottom right' }), 20, 30);
    expect(edits.map(rounded)).toEqual([
      { kind: 'resizeAnnotation', id: 3, rect: { left: 100, top: 200, width: 100, height: 70 } },
    ]);
  });

  it('keeps a stamp in proportion', async () => {
    const { target, edits, getByRole } = open([stamp]);
    press(target('Image stamp on page 1'));
    await drag(getByRole('button', { name: 'Resize from the bottom right' }), 80, 10);
    expect(edits.map(rounded)).toEqual([
      { kind: 'resizeAnnotation', id: 6, rect: { left: 100, top: 200, width: 160, height: 80 } },
    ]);
  });

  it('gives ink no handles and no Resize: the core cannot resize it', () => {
    const { target, queryAllByRole, toolbar } = open([ink]);
    press(target('Ink on page 1'));
    expect(queryAllByRole('button', { name: /^Resize/ })).toHaveLength(0);
    expect(within(toolbar()!).queryByRole('button', { name: 'Resize…' })).toBeNull();
  });
});

describe('the keyboard', () => {
  it('moves with the arrows, further with Shift, and resizes with Alt', async () => {
    const { target, edits } = open([shape]);
    const body = target('Shape on page 1');
    press(body);
    await key(body, { key: 'ArrowRight' });
    await key(body, { key: 'ArrowDown', shiftKey: true });
    await key(body, { key: 'ArrowLeft', altKey: true });
    expect(edits).toEqual([
      { kind: 'moveAnnotation', id: 3, dx: 1, dy: 0 },
      { kind: 'moveAnnotation', id: 3, dx: 0, dy: 12 },
      { kind: 'resizeAnnotation', id: 3, rect: { left: 101, top: 212, width: 79, height: 40 } },
    ]);
  });

  it('loses no press made while another is still going in', async () => {
    const answers: (() => void)[] = [];
    const base = session([shape]);
    const { target, edits } = open([shape], {
      applyEdit: async (handle, edit) => {
        await new Promise<void>((resolve) => answers.push(resolve));
        return base.engine.applyEdit(handle, edit);
      },
    });
    expect(edits).toEqual([]);
    const body = target('Shape on page 1');
    press(body);
    for (let i = 0; i < 3; i++) fireEvent.keyDown(body, { key: 'ArrowRight' });
    for (let i = 0; i < 3; i++) {
      await waitFor(() => expect(answers).toHaveLength(i + 1));
      await act(async () => answers[i]!());
    }
    expect(base.edits).toHaveLength(3);
  });

  it('moves a focused handle’s corner', async () => {
    const { target, edits, getByRole } = open([shape]);
    press(target('Shape on page 1'));
    const handle = getByRole('button', { name: 'Resize from the top left' });
    handle.focus();
    await key(handle, { key: 'ArrowLeft', shiftKey: true });
    await key(handle, { key: 'ArrowDown' });
    expect(edits).toEqual([
      { kind: 'resizeAnnotation', id: 3, rect: { left: 88, top: 200, width: 92, height: 40 } },
      { kind: 'resizeAnnotation', id: 3, rect: { left: 88, top: 201, width: 92, height: 39 } },
    ]);
    expect(document.activeElement).toBe(handle);
  });

  it('leaves the arrows to the page (scrolling) while nothing is selected', async () => {
    const { target, edits } = open([shape]);
    const body = target('Shape on page 1');
    body.focus();
    const arrow = createEvent.keyDown(body, { key: 'ArrowDown' });
    fireEvent(body, arrow);
    expect(arrow.defaultPrevented).toBe(false);
    await key(body, { key: 'Delete' });
    expect(edits).toEqual([]);

    press(body);
    const taken = createEvent.keyDown(body, { key: 'ArrowDown' });
    await act(async () => {
      fireEvent(body, taken);
    });
    expect(taken.defaultPrevented).toBe(true);
  });

  it('deletes once, however long the key is held, and keeps the focus in the pages', async () => {
    const { target, edits, toolbar, getByLabelText, queryByRole } = open([shape, stamp]);
    const body = target('Shape on page 1');
    press(body);
    fireEvent.keyDown(body, { key: 'Delete' });
    await key(body, { key: 'Delete', repeat: true });
    expect(edits).toEqual([{ kind: 'removeAnnotation', id: 3 }]);
    expect(toolbar()).toBeNull();
    expect(document.activeElement).toBe(getByLabelText('Pages'));
    expect(queryByRole('alert')).toBeNull();
    expect(target('Image stamp on page 1')).toBeTruthy();
  });

  it('asks for one removal when Delete is pressed twice before the first lands', async () => {
    const { target, edits, queryByRole } = open([shape]);
    const body = target('Shape on page 1');
    press(body);
    fireEvent.keyDown(body, { key: 'Delete' });
    await key(body, { key: 'Delete' });
    expect(edits).toHaveLength(1);
    expect(queryByRole('alert')).toBeNull();
  });
});

describe('the annotation toolbar', () => {
  it('recolours what has a colour and offers none for a stamp', async () => {
    const { target, edits, inToolbar, toolbar } = open([shape, stamp]);
    press(target('Shape on page 1'));
    expect(inToolbar('Red').getAttribute('aria-pressed')).toBe('true');
    await act(async () => {
      fireEvent.click(inToolbar('Blue'));
    });
    expect(edits).toEqual([{ kind: 'recolorAnnotation', id: 3, color: { r: 30, g: 100, b: 230 } }]);
    expect(inToolbar('Blue').getAttribute('aria-pressed')).toBe('true');
    // The same colour again is no edit.
    await act(async () => {
      fireEvent.click(inToolbar('Blue'));
    });
    expect(edits).toHaveLength(1);

    press(target('Image stamp on page 1'));
    expect(within(toolbar()!).queryByRole('button', { name: 'Blue' })).toBeNull();
    expect(within(toolbar()!).queryByRole('button', { name: 'Edit text…' })).toBeNull();
  });

  it('deletes, the focus handed to the pages even where a click focuses no button (Safari)', async () => {
    const { target, edits, inToolbar, getByLabelText } = open([shape]);
    press(target('Shape on page 1'));
    await act(async () => {
      fireEvent.click(inToolbar('Delete'));
    });
    expect(edits).toEqual([{ kind: 'removeAnnotation', id: 3 }]);
    expect(document.activeElement).toBe(getByLabelText('Pages'));
  });

  it('moves to a typed place, refusing one off the page, and gives the focus back', async () => {
    const { target, edits, inToolbar, getByRole, queryByRole } = open([shape]);
    press(target('Shape on page 1'));
    fireEvent.click(inToolbar('Move to…'));
    const dialog = getByRole('dialog', { name: 'Move to' });
    const left = within(dialog).getByLabelText('Left (pt)') as HTMLInputElement;
    const top = within(dialog).getByLabelText('Top (pt)') as HTMLInputElement;
    expect([left.value, top.value]).toEqual(['100', '200']);
    expect(document.activeElement).toBe(left);

    fireEvent.change(left, { target: { value: '600' } });
    fireEvent.submit(left.form!);
    expect(within(dialog).getByRole('alert').textContent).toBe('Left must be between 0 and 532 points.');
    expect(edits).toEqual([]);

    fireEvent.change(left, { target: { value: '50' } });
    expect(within(dialog).queryByRole('alert')).toBeNull();
    await act(async () => {
      fireEvent.submit(left.form!);
    });
    expect(edits).toEqual([{ kind: 'moveAnnotation', id: 3, dx: -50, dy: 0 }]);
    expect(queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(inToolbar('Move to…'));
  });

  it('resizes to a typed size, a stamp keeping its proportions unless told not to', async () => {
    const { target, edits, inToolbar, getByRole } = open([stamp]);
    press(target('Image stamp on page 1'));
    fireEvent.click(inToolbar('Resize…'));
    let dialog = getByRole('dialog', { name: 'Resize' });
    const width = () => within(dialog).getByLabelText('Width (pt)') as HTMLInputElement;
    const height = () => within(dialog).getByLabelText('Height (pt)') as HTMLInputElement;
    const keep = () => within(dialog).getByLabelText('Keep proportions') as HTMLInputElement;
    expect(keep().checked).toBe(true);
    fireEvent.change(width(), { target: { value: '160' } });
    expect(height().value).toBe('80');
    fireEvent.change(height(), { target: { value: '20' } });
    expect(width().value).toBe('40');
    await act(async () => {
      fireEvent.submit(width().form!);
    });
    expect(edits).toEqual([{ kind: 'resizeAnnotation', id: 6, rect: { left: 100, top: 200, width: 40, height: 20 } }]);

    fireEvent.click(inToolbar('Resize…'));
    dialog = getByRole('dialog', { name: 'Resize' });
    fireEvent.click(keep());
    fireEvent.change(width(), { target: { value: '60' } });
    expect(height().value).toBe('20');
    await act(async () => {
      fireEvent.submit(width().form!);
    });
    expect(edits[1]).toEqual({ kind: 'resizeAnnotation', id: 6, rect: { left: 100, top: 200, width: 60, height: 20 } });
  });

  it('does not keep a shape’s proportions unless asked', () => {
    const { target, inToolbar, getByLabelText } = open([shape]);
    press(target('Shape on page 1'));
    fireEvent.click(inToolbar('Resize…'));
    expect((getByLabelText('Keep proportions') as HTMLInputElement).checked).toBe(false);
  });

  it('retypes a text box, keeping the dialog open with the core’s refusal', async () => {
    const texts: string[] = [];
    const base = session([textBox]);
    const { target, inToolbar, getByRole, queryByRole, container } = open([textBox], {
      applyEdit: async (handle, edit) => {
        if (edit.kind === 'retextAnnotation') texts.push(edit.contents);
        if (edit.kind === 'retextAnnotation' && edit.contents === '日') throw new EngineError(ENCODING_GAP, 'no glyph');
        return base.engine.applyEdit(handle, edit);
      },
    });
    press(target('Text box on page 1'));
    fireEvent.click(inToolbar('Edit text…'));
    const dialog = getByRole('dialog', { name: 'Edit the text box' });
    const field = within(dialog).getByRole('textbox') as HTMLTextAreaElement;
    expect(field.value).toBe('Total');
    fireEvent.change(field, { target: { value: '日' } });
    await act(async () => {
      fireEvent.submit(field.form!);
    });
    expect(within(dialog).getByRole('alert').textContent).toBe('no glyph');
    expect(container.querySelector('.viewer__failure')).toBeNull();

    fireEvent.change(field, { target: { value: 'Sum' } });
    await act(async () => {
      fireEvent.submit(field.form!);
    });
    expect(texts).toEqual(['日', 'Sum']);
    expect(queryByRole('dialog')).toBeNull();
    expect(base.edits).toEqual([{ kind: 'retextAnnotation', id: 7, contents: 'Sum' }]);
  });

  it('takes no annotation key typed in a dialog', async () => {
    const { target, edits, inToolbar, getByRole } = open([shape]);
    press(target('Shape on page 1'));
    fireEvent.click(inToolbar('Move to…'));
    const left = within(getByRole('dialog')).getByLabelText('Left (pt)');
    await key(left, { key: 'Delete' });
    await key(left, { key: 'ArrowLeft' });
    expect(edits).toEqual([]);
  });

  it('closes an open dialog on Cancel and on Escape', () => {
    const { target, inToolbar, getByRole, queryByRole } = open([shape]);
    press(target('Shape on page 1'));
    fireEvent.click(inToolbar('Resize…'));
    fireEvent.click(within(getByRole('dialog')).getByRole('button', { name: 'Cancel' }));
    expect(queryByRole('dialog')).toBeNull();
    fireEvent.click(inToolbar('Resize…'));
    fireEvent(getByRole('dialog'), new Event('cancel', { cancelable: true }));
    expect(queryByRole('dialog')).toBeNull();
  });
});
