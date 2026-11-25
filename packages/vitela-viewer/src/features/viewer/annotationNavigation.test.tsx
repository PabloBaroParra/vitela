// Previous/Next annotation, the comments panel and "Read note" in the viewer,
// over a small in-memory engine.

import { act, cleanup, fireEvent, render, within } from '@testing-library/react';
import { useState } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Annotation, Comment, EditedDocument, Engine, PageSize, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { MarkDirtyContext } from '../save/MarkDirtyContext';
import { DocumentViewer } from './DocumentViewer';

const bitmap: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };
const pages: PageSize[] = [
  { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
  { widthPt: 792, heightPt: 612, rotationDegrees: 90 },
  { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
];
const RED = { r: 220, g: 40, b: 40 };
const box = { left: 100, top: 50, width: 80, height: 40 };
const shape: Annotation = { id: 3, pageIndex: 0, kind: 'shape', rect: box, color: RED };
const note: Annotation = { id: 5, pageIndex: 2, kind: 'note', rect: { ...box, top: 600 }, contents: 'my note\nsecond line' };
const fileComment: Comment = {
  pageIndex: 1,
  rect: { ...box, top: 400 },
  contents: 'from the file',
  author: 'Ana',
  date: 'D:20240305143000Z',
  annotationId: null,
};

function openDocument(annotations: readonly Annotation[], allowed: boolean): OpenDocument {
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
      annotationEditingAllowed: allowed,
    },
    handle: 1,
    generation: 1,
    annotations,
    canUndo: annotations.length > 0,
    canRedo: false,
  };
}

const commentsOf = (annotations: readonly Annotation[]): Comment[] => [
  fileComment,
  ...annotations.flatMap((annotation) =>
    annotation.kind === 'note'
      ? [{ ...fileComment, pageIndex: annotation.pageIndex, rect: annotation.rect, contents: annotation.contents, author: null, date: null, annotationId: annotation.id }]
      : [],
  ),
];

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

async function open(initial: readonly Annotation[], { allowed = true, ...overrides }: Partial<Engine> & { allowed?: boolean } = {}) {
  let annotations = initial;
  const answer = (): EditedDocument => ({
    info: { ...openDocument(annotations, allowed).info },
    canUndo: false,
    canRedo: false,
    annotations,
    addedAnnotationId: null,
  });
  const engine = fakeEngine({
    renderPage: async () => bitmap,
    comments: async () => commentsOf(annotations),
    applyEdit: async (_handle, edit) => {
      if (edit.kind === 'removeAnnotation') annotations = annotations.filter(({ id }) => id !== edit.id);
      return answer();
    },
    undo: async () => {
      annotations = [];
      return answer();
    },
    ...overrides,
  });
  const view = render(<Host engine={engine} initial={openDocument(initial, allowed)} />);
  await act(async () => {});
  const button = (name: string) => view.getByRole('button', { name });
  const click = (name: string) =>
    // No `focus()` first: Safari does not focus a clicked button, and neither does this.
    act(async () => {
      fireEvent.click(button(name));
    });
  const scroller = view.container.querySelector<HTMLElement>('.viewer__scroller')!;
  const status = () => view.container.querySelector('.annotation-nav__status')!.textContent;
  const panel = () => view.queryByRole('complementary', { name: 'Comments' });
  const rows = () => [...(panel()?.querySelectorAll('.comment-row__go') ?? [])].map((row) => row.textContent);
  const ring = () => view.container.querySelector<HTMLElement>('.annotation-ring');
  return { ...view, button, click, scroller, status, panel, rows, ring };
}

beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
beforeEach(() => {
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockReturnValue(1200);
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(600);
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ putImageData: () => {} } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal('ImageData', class {});
});
afterEach(cleanup);

describe('Previous and Next annotation', () => {
  it('visits session and file annotations in page order, wrapping, and says where it is', async () => {
    const { click, status } = await open([note, shape]);
    const visited = [];
    for (let i = 0; i < 4; i++) {
      await click('Next annotation');
      visited.push(status());
    }
    expect(visited).toEqual([
      'Shape on page 1, 1 of 3',
      'Comment on page 2, 2 of 3',
      'Note on page 3, 3 of 3',
      'Shape on page 1, 1 of 3',
    ]);
    await click('Previous annotation');
    expect(status()).toBe('Note on page 3, 3 of 3');
  });

  it('selects a session annotation and only marks a file comment', async () => {
    const { click, button, queryByRole, ring } = await open([shape]);
    await click('Next annotation');
    expect(button('Shape on page 1').getAttribute('aria-pressed')).toBe('true');
    expect(ring()).toBeNull();
    await click('Next annotation');
    expect(queryByRole('toolbar', { name: 'Annotation' })).toBeNull();
    expect(ring()).not.toBeNull();
    // The focus stays: the next press goes on.
    expect(document.activeElement).toBe(button('Next annotation'));
  });

  it('scrolls to one off screen, on a turned page too', async () => {
    const { click, scroller } = await open([note]);
    await click('Next annotation');
    const scale = 96 / 72;
    const turnedTop = 16 + 792 * scale + 16 + 400 * scale;
    expect(scroller.scrollTop).toBeGreaterThan(turnedTop - 600);
    expect(scroller.scrollTop).toBeLessThan(turnedTop);
    await click('Next annotation');
    expect(scroller.scrollTop).toBeGreaterThan(turnedTop + 600);
  });

  it('goes on from the annotation selected by hand', async () => {
    const { click, button, status } = await open([shape, note]);
    await click('Next annotation');
    await click('Next annotation');
    fireEvent.click(button('Shape on page 1'));
    await click('Next annotation');
    expect(status()).toBe('Comment on page 2, 2 of 3');
  });

  it('works where annotating is forbidden: reveals without selecting', async () => {
    const { click, status, ring, queryByRole } = await open([shape], { allowed: false });
    await click('Next annotation');
    expect(status()).toBe('Shape on page 1, 1 of 2');
    expect(ring()).not.toBeNull();
    expect(queryByRole('toolbar', { name: 'Annotation' })).toBeNull();
    await click('Next annotation');
    expect(status()).toBe('Comment on page 2, 2 of 2');
  });

  it('reveals without taking an armed tool away', async () => {
    const { click, button, ring } = await open([shape]);
    await click('Ink');
    await click('Next annotation');
    expect(button('Ink').getAttribute('aria-pressed')).toBe('true');
    expect(ring()).not.toBeNull();
  });

  it('drops the mark on a press on a page or Escape in the pages, and keeps its place', async () => {
    const { click, container, ring, scroller, status } = await open([shape], { allowed: false });
    await click('Next annotation');
    fireEvent.pointerDown(container.querySelector('[data-page="1"] .text-layer')!, { button: 0, isPrimary: true });
    expect(ring()).toBeNull();
    await click('Next annotation');
    expect(status()).toBe('Comment on page 2, 2 of 2');
    fireEvent.keyDown(scroller, { key: 'Escape' });
    expect(ring()).toBeNull();
  });

  it('does not bring the mark back once its annotation was selected and let go', async () => {
    const { click, button, ring } = await open([shape]);
    await click('Ink');
    await click('Next annotation');
    expect(ring()).not.toBeNull();
    await click('Select');
    const target = button('Shape on page 1');
    fireEvent.keyDown(target, { key: 'Enter' });
    expect(target.getAttribute('aria-pressed')).toBe('true');
    expect(ring()).toBeNull();
    fireEvent.keyDown(target, { key: 'Enter' });
    expect(target.getAttribute('aria-pressed')).toBe('false');
    expect(ring()).toBeNull();
  });

  it('keeps the focus on Next where the browser does not focus a clicked button', async () => {
    const { click, button } = await open([shape]);
    await click('Next annotation');
    expect(document.activeElement).toBe(button('Next annotation'));
    await click('Previous annotation');
    expect(document.activeElement).toBe(button('Previous annotation'));
  });

  it('reveals clear of the toolbar that floats over the foot of the pages', async () => {
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      if (this.matches('.annotation-toolbar')) return new DOMRect(0, 150, 1200, 440);
      return new DOMRect(0, 0, 1200, 600);
    });
    // Drawn 800 to 853 px down page 1: in the bare viewport once centred, but under the toolbar.
    const low: Annotation = { ...shape, rect: { ...box, top: 600 } };
    const { click, scroller } = await open([low], { comments: async () => [] });
    await click('Next annotation');
    const top = 16 + 600 * (96 / 72) - scroller.scrollTop;
    expect(top).toBeGreaterThanOrEqual(0);
    expect(top + 40 * (96 / 72)).toBeLessThanOrEqual(150);
    vi.mocked(HTMLElement.prototype.getBoundingClientRect).mockRestore();
  });

  it('marks no more than the page for a file annotation that sticks out of it', async () => {
    const huge: Comment = { ...fileComment, pageIndex: 0, rect: { left: -500, top: 700, width: 5000, height: 9000 } };
    const { click, ring } = await open([], { comments: async () => [huge] });
    await click('Next annotation');
    const { left, top, width, height } = ring()!.style;
    const px = [left, top, width, height].map((length) => Math.round(parseFloat(length) * 0.75));
    expect(px).toEqual([0, 700, 612, 92]);
  });

  it('does nothing with nothing to visit', async () => {
    const { button, click, ring, status, scroller, queryByRole } = await open([], { comments: async () => [] });
    expect(button('Next annotation').getAttribute('aria-disabled')).toBe('true');
    expect(button('Previous annotation').getAttribute('aria-disabled')).toBe('true');
    await click('Next annotation');
    await click('Previous annotation');
    expect([ring(), status(), scroller.scrollTop]).toEqual([null, '', 0]);
    expect(queryByRole('toolbar', { name: 'Annotation' })).toBeNull();
  });
});

describe('the comments panel', () => {
  it('lists file comments and session notes by page', async () => {
    const { click, button, panel, rows } = await open([note]);
    expect(panel()).toBeNull();
    expect(button('Comments').getAttribute('aria-expanded')).toBe('false');
    await click('Comments');
    expect(button('Comments').getAttribute('aria-expanded')).toBe('true');
    expect(button('Comments').getAttribute('aria-controls')).toBe(panel()!.id);
    expect(within(panel()!).getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual(['Page 2', 'Page 3']);
    expect(rows()[0]).toContain('from the file');
    expect(rows()[0]).toContain('Ana · 2024-03-05 14:30');
    expect(rows()[1]).toContain('my note');
  });

  it('follows a delete', async () => {
    const { click, rows, button } = await open([note]);
    await click('Comments');
    expect(rows()).toHaveLength(2);
    await click('Next annotation');
    await click('Next annotation');
    await act(async () => fireEvent.click(button('Delete')));
    expect(rows()).toHaveLength(1);
  });

  it('follows an undo', async () => {
    const { click, rows } = await open([note]);
    await click('Comments');
    await click('Undo');
    expect(rows()).toHaveLength(1);
  });

  it('goes to the comment, marks its row and goes on from it', async () => {
    const { click, panel, button, status } = await open([note]);
    await click('Comments');
    const go = () => [...panel()!.querySelectorAll<HTMLElement>('.comment-row__go')];
    await act(async () => fireEvent.click(go()[1]!));
    expect(go().map((row) => row.getAttribute('aria-current'))).toEqual([null, 'true']);
    expect(button('Note on page 3').getAttribute('aria-pressed')).toBe('true');
    expect(status()).toBe('Note on page 3, 2 of 2');
    await click('Next annotation');
    expect(go().map((row) => row.getAttribute('aria-current'))).toEqual(['true', null]);
  });

  it('says so when there are none', async () => {
    const { click, panel } = await open([], { comments: async () => [] });
    await click('Comments');
    expect(panel()!.textContent).toContain('no comments');
  });

  it('says why it failed, and tries again', async () => {
    const comments = vi.fn().mockRejectedValueOnce(new Error('broken')).mockResolvedValue([fileComment]);
    const { click, panel, rows } = await open([], { comments });
    await click('Comments');
    expect(within(panel()!).getByRole('alert').textContent).toContain('broken');
    await click('Try again');
    expect(rows()).toHaveLength(1);
    expect(document.activeElement).toBe(panel());
  });

  it('says beside its toggle that the comments failed while it is closed, and only then', async () => {
    const comments = vi.fn().mockRejectedValue(new Error('broken'));
    const { click, button, container, queryAllByRole, panel } = await open([], { comments });
    const notice = () => container.querySelector('.annotation-nav__failed')!.textContent;
    expect(notice()).toBe('Could not list the comments. Open Comments to try again.');
    expect(container.querySelector('.annotation-nav__failed')!.getAttribute('role')).toBe('status');
    expect(button('Comments').getAttribute('aria-describedby')).toBe(container.querySelector('.annotation-nav__failed')!.id);
    expect(button('Comments').hasAttribute('data-failed')).toBe(true);
    await click('Comments');
    expect(notice()).toBe('');
    expect(button('Comments').hasAttribute('aria-describedby')).toBe(false);
    expect(queryAllByRole('alert')).toHaveLength(1);
    expect(within(panel()!).getByRole('alert').textContent).toContain('broken');
  });

  it('closes with Escape or its button and hands the focus to its toggle', async () => {
    const { click, panel, button } = await open([note]);
    await click('Comments');
    fireEvent.keyDown(panel()!.querySelector('.comment-row__go')!, { key: 'Escape' });
    expect(panel()).toBeNull();
    expect(document.activeElement).toBe(button('Comments'));
    await click('Comments');
    await click('Close comments');
    expect(panel()).toBeNull();
    expect(document.activeElement).toBe(button('Comments'));
  });
});

describe('Read note', () => {
  it('shows the whole text read-only, with who wrote it, and gives the focus back', async () => {
    const { click, getByRole, queryByRole, button } = await open([note], { allowed: false });
    await click('Comments');
    const name = 'Read note on page 2: from the file';
    await click(name);
    const dialog = getByRole('dialog', { name: 'Note on page 2' });
    const text = within(dialog).getByRole<HTMLTextAreaElement>('textbox');
    expect([text.value, text.readOnly]).toEqual(['from the file', true]);
    expect(dialog.textContent).toContain('Ana · 2024-03-05 14:30');
    expect(document.activeElement).toBe(text);
    await act(async () => fireEvent.click(within(dialog).getByRole('button', { name: 'Close' })));
    expect(queryByRole('dialog')).toBeNull();
    expect(document.activeElement).toBe(button(name));
  });

  it('closes on Escape', async () => {
    const { click, getByRole, queryByRole } = await open([note]);
    await click('Comments');
    await click('Read note on page 3: my note second line');
    const dialog = getByRole('dialog');
    expect(within(dialog).getByRole<HTMLTextAreaElement>('textbox').value).toBe('my note\nsecond line');
    act(() => {
      dialog.dispatchEvent(new Event('cancel', { cancelable: true }));
    });
    expect(queryByRole('dialog')).toBeNull();
  });

  it('is offered for a selected note, and for nothing else', async () => {
    const { click, getByRole, queryByRole } = await open([shape, note]);
    await click('Next annotation');
    expect(queryByRole('button', { name: 'Read note…' })).toBeNull();
    await click('Previous annotation');
    await click('Read note…');
    expect(within(getByRole('dialog', { name: 'Note on page 3' })).getByRole<HTMLTextAreaElement>('textbox').value).toBe(
      'my note\nsecond line',
    );
  });
});
