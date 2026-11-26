// The forms panel in the viewer, over a small in-memory engine: filling in
// goes through the edit queue (undo included), paints the page again, and a
// field focused in the panel is shown on its page.

import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EDIT_NOT_PERMITTED, EngineError } from '@vitela/core';
import type { EditedDocument, Engine, FormField, PageSize, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { ALL_FIELDS, NAME } from '../../test/formFields';
import type { OpenDocument } from '../document/useDocument';
import { MarkDirtyContext, SaveWaitsForEditsContext } from '../save/MarkDirtyContext';
import { DocumentViewer } from './DocumentViewer';

const bitmap: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };
const pages: PageSize[] = [
  { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
  { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
];

function info(formFillAllowed: boolean) {
  return {
    pageCount: pages.length,
    pages,
    pageIds: pages.map((_, index) => index),
    textExtractionAllowed: true,
    documentAssemblyAllowed: true,
    blocks: [],
    importRefusal: null,
    annotationEditingAllowed: formFillAllowed,
    formFillAllowed,
  };
}

interface HostProps {
  readonly engine: Engine;
  readonly initial: OpenDocument;
  readonly onDirty: () => void;
  readonly waitForEdits: (settled: () => Promise<unknown>) => () => void;
}

function Host({ engine, initial, onDirty, waitForEdits }: HostProps) {
  const [document, setDocument] = useState(initial);
  return (
    <SaveWaitsForEditsContext.Provider value={waitForEdits}>
      <MarkDirtyContext.Provider value={onDirty}>
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
    </SaveWaitsForEditsContext.Provider>
  );
}

async function open({ allowed = true, ...overrides }: Partial<Engine> & { allowed?: boolean } = {}) {
  let fields: readonly FormField[] = ALL_FIELDS;
  const history: (readonly FormField[])[] = [];
  const answer = (): EditedDocument => ({
    info: info(allowed),
    canUndo: history.length > 0,
    canRedo: false,
    annotations: [],
    addedAnnotationId: null,
    formFields: fields,
  });
  const renderPage = vi.fn(async (_handle: number, _index: number, _dpi: number) => bitmap);
  const applyEdit = vi.fn(async (_handle: number, edit: Parameters<Engine['applyEdit']>[1]) => {
    if (edit.kind !== 'setFieldValue') throw new Error('not a fill');
    history.push(fields);
    fields = fields.map((field) =>
      field.id !== edit.fieldId
        ? field
        : field.kind === 'text' && edit.value.kind === 'text'
          ? { ...field, value: edit.value.text }
          : field,
    );
    return answer();
  });
  const engine = fakeEngine({
    renderPage,
    comments: async () => [],
    formFields: async () => [...fields],
    applyEdit,
    undo: async () => {
      fields = history.pop() ?? fields;
      return answer();
    },
    ...overrides,
  });
  const onDirty = vi.fn();
  /** The edit queue the viewer handed to the save, while it is mounted. */
  let settled: (() => Promise<unknown>) | null = null;
  const waitForEdits = (handed: () => Promise<unknown>) => {
    settled = handed;
    return () => {
      if (settled === handed) settled = null;
    };
  };
  const initial: OpenDocument = { name: 'form.pdf', info: info(allowed), handle: 1, generation: 1 };
  const view = render(<Host engine={engine} initial={initial} onDirty={onDirty} waitForEdits={waitForEdits} />);
  await act(async () => {});
  const button = (name: string) => view.getByRole('button', { name });
  const click = (name: string) =>
    act(async () => {
      fireEvent.click(button(name));
    });
  const panel = () => view.queryByRole('complementary', { name: 'Form fields' });
  const scroller = view.container.querySelector<HTMLElement>('.viewer__scroller')!;
  const ring = () => view.container.querySelector<HTMLElement>('.annotation-ring');
  return { ...view, button, click, panel, scroller, ring, renderPage, applyEdit, onDirty, settled: () => settled };
}

beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1200);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ putImageData: () => {} } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
});
afterEach(cleanup);

describe('the forms panel', () => {
  it('opens from the Forms button, which says it controls it, and closes back to it', async () => {
    const { click, button, panel, getByLabelText } = await open();
    const toggle = button('Forms');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    await click('Forms');
    expect(panel()).not.toBeNull();
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(toggle.getAttribute('aria-controls')).toBe(panel()!.id);
    expect((getByLabelText('full_name') as HTMLInputElement).value).toBe('Ada Lovelace');
    await click('Close form fields');
    expect(panel()).toBeNull();
    expect(document.activeElement).toBe(toggle);
  });

  it('shares the side of the pages with the comments: one panel at a time', async () => {
    const { click, panel, queryByRole } = await open();
    await click('Comments');
    expect(queryByRole('complementary', { name: 'Comments' })).not.toBeNull();
    await click('Forms');
    expect(panel()).not.toBeNull();
    expect(queryByRole('complementary', { name: 'Comments' })).toBeNull();
    await click('Comments');
    expect(panel()).toBeNull();
  });

  it('fills a field in as one undoable edit, marks the document dirty and paints its page again', async () => {
    const { click, getByLabelText, applyEdit, renderPage, onDirty } = await open();
    await click('Forms');
    const rendered = renderPage.mock.calls.filter(([, index]) => index === 0).length;
    const name = getByLabelText('full_name') as HTMLInputElement;
    fireEvent.focus(name);
    fireEvent.change(name, { target: { value: 'Grace Hopper' } });
    await act(async () => {
      fireEvent.blur(name);
    });
    expect(applyEdit).toHaveBeenCalledTimes(1);
    expect(applyEdit.mock.calls[0]![1]).toEqual({ kind: 'setFieldValue', fieldId: NAME.id, value: { kind: 'text', text: 'Grace Hopper' } });
    expect(onDirty).toHaveBeenCalledTimes(1);
    expect(renderPage.mock.calls.filter(([, index]) => index === 0).length).toBe(rendered + 1);
    // Page 2's fields did not change: it is not painted again.
    expect(renderPage.mock.calls.filter(([, index]) => index === 1).length).toBeLessThanOrEqual(1);

    await click('Undo');
    expect(name.value).toBe('Ada Lovelace');
    expect(renderPage.mock.calls.filter(([, index]) => index === 0).length).toBe(rendered + 2);
  });

  it('hands its edit queue to the save, which settles only once a fill still queued has landed', async () => {
    let land: () => void = () => {};
    const landed = new Promise<void>((resolve) => (land = resolve));
    const { click, getByLabelText, applyEdit, settled, unmount } = await open();
    const fill = applyEdit.getMockImplementation()!;
    applyEdit.mockImplementation(async (...args) => (await landed, fill(...args)));
    await click('Forms');
    const name = getByLabelText('full_name');
    fireEvent.change(name, { target: { value: 'Grace Hopper' } });
    await act(async () => {
      fireEvent.blur(name);
    });
    let done = false;
    void settled()!().then(() => (done = true));
    await act(async () => {});
    expect(done).toBe(false);
    await act(async () => land());
    expect(done).toBe(true);
    unmount();
    expect(settled()).toBeNull();
  });

  it('sends nothing for a field left as it was', async () => {
    const { click, getByLabelText, applyEdit } = await open();
    await click('Forms');
    const name = getByLabelText('full_name');
    fireEvent.focus(name);
    fireEvent.change(name, { target: { value: 'Ada Lovelace' } });
    await act(async () => {
      fireEvent.blur(name);
    });
    await act(async () => {
      fireEvent.change(getByLabelText('country'), { target: { value: 'Uruguay' } });
    });
    expect(applyEdit).not.toHaveBeenCalled();
  });

  it('shows the focused field on its page: scrolls to it and rings it', async () => {
    const { click, getByLabelText, scroller, ring } = await open();
    await click('Forms');
    fireEvent.focus(getByLabelText('country'));
    // Page 2 starts 16 + 792 * 4/3 + 16 px down; the field 330 pt into it.
    const fieldTop = 16 + 792 * (96 / 72) + 16 + 330 * (96 / 72);
    expect(scroller.scrollTop).toBeGreaterThan(fieldTop - 600);
    expect(scroller.scrollTop).toBeLessThan(fieldTop);
    await act(async () => {});
    expect(ring()).not.toBeNull();
    expect(ring()!.closest('[data-page]')!.getAttribute('data-page')).toBe('2');
    fireEvent.blur(getByLabelText('country'));
    expect(ring()).toBeNull();
  });

  it('is read-only where the document does not allow filling in', async () => {
    const { click, getByLabelText, getByText, applyEdit } = await open({ allowed: false });
    await click('Forms');
    expect(getByText('This document does not allow its form fields to be filled in.')).toBeTruthy();
    expect(getByLabelText('full_name').hasAttribute('readonly')).toBe(true);
    expect((getByLabelText('subscribe') as HTMLInputElement).disabled).toBe(true);
    expect(applyEdit).not.toHaveBeenCalled();
  });

  it('turns read-only when the core refuses a fill, without a failure over the pages', async () => {
    const refuse = async () => {
      throw new EngineError(EDIT_NOT_PERMITTED, 'not permitted');
    };
    const { click, getByLabelText, getByText, container } = await open({ applyEdit: refuse });
    await click('Forms');
    await act(async () => {
      fireEvent.click(getByLabelText('subscribe'));
    });
    expect(getByText('This document does not allow its form fields to be filled in.')).toBeTruthy();
    expect((getByLabelText('subscribe') as HTMLInputElement).checked).toBe(true);
    expect(container.querySelector('.viewer__failure')).toBeNull();
  });

  it('says next to the field why the core refused its value', async () => {
    const refuse = async () => {
      throw new EngineError('UnsupportedOperation', 'text longer than /MaxLen');
    };
    const { click, getByLabelText, getByRole } = await open({ applyEdit: refuse });
    await click('Forms');
    const name = getByLabelText('full_name');
    fireEvent.change(name, { target: { value: 'far too long' } });
    await act(async () => {
      fireEvent.blur(name);
    });
    expect(getByRole('alert').textContent).toBe('text longer than /MaxLen');
    expect(name.getAttribute('aria-describedby')).toBe(getByRole('alert').id);
  });
});
