import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { useState } from 'react';
import { EngineError } from '@vitela/core';
import type { DocumentBlock, DocumentInfo, EditedDocument, Engine, PageEdit, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { MarkDirtyContext } from '../save/MarkDirtyContext';
import { CARD, GRID_GAP, GRID_PADDING } from './gridLayout';
import { OrganizeScreen } from './OrganizeScreen';

/** Room for exactly four cards a row. */
const VIEWPORT = { width: 2 * GRID_PADDING + 4 * CARD.width + 3 * GRID_GAP, height: 600 };
const bitmap: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };

/** One page of the fake document: its id, turn, and the PDF it came from (0: the opened one). */
interface FakePage {
  readonly id: number;
  readonly turn: number;
  readonly source: number;
}

function info(pages: readonly FakePage[], allowed = true, importRefusal: string | null = null): DocumentInfo {
  const blocks: DocumentBlock[] = [];
  pages.forEach((page, index) => {
    const last = blocks.at(-1);
    const source: DocumentBlock['source'] = page.source === 0 ? { kind: 'base' } : { kind: 'imported', id: page.source };
    const same = last && (last.source.kind === 'base' ? 0 : (last.source as { id: number }).id) === page.source;
    if (same) blocks[blocks.length - 1] = { ...last, count: last.count + 1 };
    else blocks.push({ source, part: null, start: index, count: 1 });
  });
  return {
    pageCount: pages.length,
    pages: pages.map(({ turn }) => ({ widthPt: 612, heightPt: 792, rotationDegrees: turn })),
    pageIds: pages.map(({ id }) => id),
    textExtractionAllowed: true,
    documentAssemblyAllowed: allowed,
    blocks,
    importRefusal: allowed ? importRefusal : (importRefusal ?? 'this document does not permit inserting pages'),
    annotationEditingAllowed: true,
    formFillAllowed: true,
  };
}

/**
 * An engine over a page list, applying edits and imports the way the core
 * does. A prepared file adds one page per byte; one whose first byte is 0xee
 * is locked under "secret".
 */
function pagesEngine(count: number, allowed = true, importRefusal: string | null = null) {
  let pages: FakePage[] = Array.from({ length: count }, (_, id) => ({ id, turn: 0, source: 0 }));
  const undo: FakePage[][] = [];
  const redo: FakePage[][] = [];
  const edits: PageEdit[] = [];
  const prepared = new Map<number, number>();
  const imports: [readonly number[], number][] = [];
  let nextPrepared = 1;
  let nextSource = 1;
  let nextPageId = 100;
  /** While set, every page edit waits for it before it lands. */
  let held: Promise<void> | null = null;
  const answer = (): EditedDocument => ({ info: info(pages, allowed, importRefusal), canUndo: undo.length > 0, canRedo: redo.length > 0, annotations: [], addedAnnotationId: null, formFields: [] });
  const record = (next: FakePage[]) => {
    undo.push(pages);
    redo.length = 0;
    pages = next;
  };
  const engine: Engine = fakeEngine({
    renderPage: async () => bitmap,
    applyEdit: async (_handle, anyEdit) => {
      const edit = anyEdit as PageEdit;
      edits.push(edit);
      if (held) await held;
      if (!allowed) {
        const error = new Error('the document does not permit page assembly');
        error.name = 'EditNotPermitted';
        throw error;
      }
      const next = [...pages];
      if (edit.kind === 'move') next.splice(edit.to, 0, ...next.splice(edit.from, edit.count));
      else if (edit.kind === 'rotate')
        for (let i = edit.from; i < edit.from + edit.count; i++)
          next[i] = { ...next[i]!, turn: (next[i]!.turn + edit.degrees + 360) % 360 };
      else next.splice(edit.from, edit.count);
      record(next);
      return answer();
    },
    undo: async () => {
      redo.push(pages);
      pages = undo.pop()!;
      return answer();
    },
    redo: async () => {
      undo.push(pages);
      pages = redo.pop()!;
      return answer();
    },
    prepareImport: async (bytes, password) => {
      if (bytes[0] === 0xee && password !== 'secret') {
        throw new EngineError(password === undefined ? 'PasswordRequired' : 'WrongPassword', 'password needed');
      }
      const id = nextPrepared++;
      prepared.set(id, bytes.length);
      return { id, pageCount: bytes.length, warnings: [] };
    },
    discardImport: async (id) => void prepared.delete(id),
    importPrepared: async (_handle, ids, index) => {
      imports.push([ids, index]);
      const added: FakePage[] = [];
      const sourceIds = ids.map((id) => {
        const source = nextSource++;
        for (let i = 0; i < prepared.get(id)!; i++) added.push({ id: nextPageId++, turn: 0, source });
        prepared.delete(id);
        return source;
      });
      const next = [...pages];
      next.splice(index, 0, ...added);
      record(next);
      return { ...answer(), sourceIds };
    },
  });
  /** Holds page edits in flight until the returned release is called. */
  const holdEdits = () => {
    let release!: () => void;
    held = new Promise<void>((resolve) => (release = resolve));
    return () => {
      held = null;
      release();
    };
  };
  return { engine, edits, imports, prepared, opened: answer().info, holdEdits };
}

function Harness({ engine, initial, markDirty }: { engine: Engine; initial: DocumentInfo; markDirty: () => void }) {
  const [document, setDocument] = useState<OpenDocument>({ name: 'a.pdf', info: initial, handle: 1, generation: 1 });
  return (
    <MarkDirtyContext.Provider value={markDirty}>
      <OrganizeScreen
        engine={engine}
        document={document}
        onEdited={(_handle, edited) => {
          setDocument((d) => ({ ...d, info: edited.info, canUndo: edited.canUndo, canRedo: edited.canRedo }));
          return true;
        }}
        onSourcesNamed={(_handle, names) =>
          setDocument((d) => ({ ...d, sourceNames: new Map([...(d.sourceNames ?? []), ...names]) }))
        }
      />
    </MarkDirtyContext.Provider>
  );
}

function setup(count: number, allowed = true, importRefusal: string | null = null) {
  const fake = pagesEngine(count, allowed, importRefusal);
  const markDirty = vi.fn();
  const view = render(<Harness engine={fake.engine} initial={fake.opened} markDirty={markDirty} />);
  const grid = () => screen.getByRole('listbox');
  const cards = () => [...view.container.querySelectorAll<HTMLElement>('.page-card')];
  /** Page ids in grid order. */
  const order = () => cards().map((card) => Number(card.dataset.pageId));
  const selected = () => cards().filter((card) => card.getAttribute('aria-selected') === 'true').map((c) => Number(c.dataset.index));
  /** Block cards as "label: pages". */
  const blocks = () => cards().map((card) => card.getAttribute('aria-label'));
  return { ...fake, ...view, markDirty, grid, cards, order, selected, blocks };
}

/** Files with one page per byte. */
const pdf = (name: string, pages = 1, first = 1) =>
  new File([new Uint8Array(Array.from({ length: pages }, (_, i) => (i === 0 ? first : 1)))], name, {
    type: 'application/pdf',
  });

async function addPdfs(files: File[]) {
  await act(async () => fireEvent.change(screen.getByLabelText('Add PDFs'), { target: { files } }));
}

const button = (name: string) => screen.getByRole('button', { name });

// jsdom has no modal <dialog>: model just enough of it for the password prompt.
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
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('OrganizeScreen', () => {
  it('shows a card per page with its thumbnail, the first one selected', async () => {
    const { cards, selected } = setup(5);
    expect(cards()).toHaveLength(5);
    expect(selected()).toEqual([0]);
    await waitFor(() => expect(cards().every((card) => card.dataset.state === 'rendered')).toBe(true));
  });

  it('mounts only the rows near the viewport', () => {
    // 4 a row, 600 px tall: about three rows show, three more of overscan.
    const { cards } = setup(400);
    expect(cards().length).toBeLessThan(40);
  });

  it('rotates the selected pages as one edit and marks the document dirty', async () => {
    const { edits, cards, markDirty, grid } = setup(4);
    fireEvent.keyDown(grid(), { key: 'ArrowRight', shiftKey: true });
    await act(async () => fireEvent.click(button('Rotate right')));
    expect(edits).toEqual([{ kind: 'rotate', from: 0, count: 2, degrees: 90 }]);
    expect(cards().map((card) => card.dataset.rotation)).toEqual(['90', '90', '0', '0']);
    expect(markDirty).toHaveBeenCalledTimes(1);

    await act(async () => fireEvent.click(button('Rotate left')));
    expect(edits[1]).toEqual({ kind: 'rotate', from: 0, count: 2, degrees: -90 });
  });

  it('selects with the arrows and Shift, and moves the selection with Ctrl+Shift+arrow', async () => {
    const { edits, order, selected, grid } = setup(5);
    fireEvent.keyDown(grid(), { key: 'ArrowRight' });
    fireEvent.keyDown(grid(), { key: 'ArrowRight', shiftKey: true });
    expect(selected()).toEqual([1, 2]);

    await act(async () => fireEvent.keyDown(grid(), { key: 'ArrowLeft', ctrlKey: true, shiftKey: true }));
    expect(edits).toEqual([{ kind: 'move', from: 1, count: 2, to: 0 }]);
    expect(order()).toEqual([1, 2, 0, 3, 4]);
    // The selection follows the moved pages.
    expect(selected()).toEqual([0, 1]);

    await act(async () => fireEvent.click(button('Move later')));
    expect(order()).toEqual([0, 1, 2, 3, 4]);
    expect(selected()).toEqual([1, 2]);
  });

  it('deletes the selection with the Delete key and selects the page after it', async () => {
    const { edits, order, selected, grid } = setup(4);
    fireEvent.keyDown(grid(), { key: 'ArrowRight' });
    await act(async () => fireEvent.keyDown(grid(), { key: 'Delete' }));
    expect(edits).toEqual([{ kind: 'remove', from: 1, count: 1 }]);
    expect(order()).toEqual([0, 2, 3]);
    expect(selected()).toEqual([1]);
  });

  it('undoes and redoes with the buttons and Ctrl+Z / Ctrl+Shift+Z', async () => {
    const { order, grid } = setup(3);
    expect(button('Undo').getAttribute('aria-disabled')).toBe('true');
    await act(async () => fireEvent.click(button('Delete')));
    expect(order()).toEqual([1, 2]);

    await act(async () => fireEvent.keyDown(grid(), { key: 'z', ctrlKey: true }));
    expect(order()).toEqual([0, 1, 2]);
    expect(button('Redo').getAttribute('aria-disabled')).toBe('false');
    await act(async () => fireEvent.keyDown(grid(), { key: 'Z', ctrlKey: true, shiftKey: true }));
    expect(order()).toEqual([1, 2]);
    await act(async () => fireEvent.click(button('Undo')));
    expect(order()).toEqual([0, 1, 2]);
  });

  it('reads and labels the history keys as the viewer does', async () => {
    const { order, grid } = setup(3);
    expect(button('Undo').getAttribute('aria-keyshortcuts')).toBe('Control+Z');
    expect(button('Redo').getAttribute('aria-keyshortcuts')).toBe('Control+Shift+Z Control+Y');
    await act(async () => fireEvent.click(button('Delete')));
    await act(async () => fireEvent.keyDown(grid(), { key: 'z', ctrlKey: true }));
    expect(order()).toEqual([0, 1, 2]);
    await act(async () => fireEvent.keyDown(grid(), { key: 'y', ctrlKey: true }));
    expect(order()).toEqual([1, 2]);
  });

  it('takes no history key while a modal dialog is open', async () => {
    const { order, grid } = setup(3);
    await act(async () => fireEvent.click(button('Delete')));
    const modal = document.body.appendChild(document.createElement('dialog'));
    modal.setAttribute('open', '');
    modal.setAttribute('aria-modal', 'true');
    try {
      await act(async () => fireEvent.keyDown(grid(), { key: 'z', ctrlKey: true }));
      expect(order()).toEqual([1, 2]);
    } finally {
      modal.remove();
    }
  });

  it('drags the selection to a new place', async () => {
    const { edits, order, cards, grid } = setup(4);
    const step = CARD.width + GRID_GAP;
    const middleOf = (column: number) => GRID_PADDING + column * step + CARD.width / 2;
    const y = GRID_PADDING + CARD.height / 2;

    fireEvent.pointerDown(cards()[0]!, { pointerId: 1, button: 0, clientX: middleOf(0), clientY: y });
    fireEvent.pointerMove(grid(), { pointerId: 1, clientX: middleOf(1), clientY: y });
    expect(grid().dataset.dragging).toBe('true');
    // Right of page 3's middle: in front of page 4.
    fireEvent.pointerMove(grid(), { pointerId: 1, clientX: middleOf(2) + 10, clientY: y });
    await act(async () => fireEvent.pointerUp(grid(), { pointerId: 1, clientX: middleOf(2) + 10, clientY: y }));
    expect(edits).toEqual([{ kind: 'move', from: 0, count: 1, to: 2 }]);
    expect(order()).toEqual([1, 2, 0, 3]);
    expect(grid().dataset.dragging).toBe('false');
  });

  it('after a drag whose click never reached a card, the next click on a card still selects', async () => {
    const { selected, cards, grid } = setup(4);
    fireEvent.pointerDown(cards()[0]!, { pointerId: 1, button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(grid(), { pointerId: 1, clientX: 60, clientY: 10 });
    await act(async () => fireEvent.pointerUp(grid(), { pointerId: 1, clientX: 60, clientY: 10 }));
    // The browser's click after a captured drag went to the grid, not a card.
    fireEvent.click(grid());
    fireEvent.pointerDown(cards()[3]!, { pointerId: 2, button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(grid(), { pointerId: 2, clientX: 10, clientY: 10 });
    fireEvent.click(cards()[3]!);
    expect(selected()).toEqual([3]);
  });

  it('a click that does not travel only selects', async () => {
    const { edits, selected, cards, grid } = setup(4);
    fireEvent.pointerDown(cards()[2]!, { pointerId: 1, button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(grid(), { pointerId: 1, clientX: 11, clientY: 10 });
    fireEvent.click(cards()[2]!);
    fireEvent.click(cards()[3]!, { shiftKey: true });
    expect(selected()).toEqual([2, 3]);
    expect(edits).toEqual([]);
  });

  it('adds picked PDFs after the last page as one import, selects them and marks the document dirty', async () => {
    const { imports, order, selected, markDirty } = setup(2);
    await addPdfs([pdf('b.pdf', 2), pdf('c.pdf')]);
    expect(imports).toEqual([[[1, 2], 2]]);
    expect(order()).toEqual([0, 1, 100, 101, 102]);
    expect(selected()).toEqual([2, 3, 4]);
    expect(markDirty).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Added 3 pages from 2 PDFs.')).toBeTruthy();

    // One undo takes the whole pick back.
    await act(async () => fireEvent.click(button('Undo')));
    expect(order()).toEqual([0, 1]);
  });

  it('asks a locked PDF for its password before adding it', async () => {
    const { imports, order } = setup(1);
    await addPdfs([pdf('locked.pdf', 1, 0xee)]);
    const dialog = await screen.findByRole('dialog', { name: 'Password required' });
    expect(dialog.textContent).toContain('locked.pdf is protected. Enter its password to add its pages.');
    fireEvent.change(screen.getByLabelText('Password'), { target: { value: 'secret' } });
    await act(async () => fireEvent.click(screen.getByRole('button', { name: 'Add' })));
    await waitFor(() => expect(imports).toHaveLength(1));
    expect(order()).toEqual([0, 100]);
  });

  it('adds PDFs dropped on the screen and claims the drop so the workspace opens nothing', async () => {
    const { imports, container } = setup(1);
    const files = [pdf('dropped.pdf')];
    const drop = new Event('drop', { bubbles: true, cancelable: true });
    Object.defineProperty(drop, 'dataTransfer', { value: { types: ['Files'], files } });
    await act(async () => void container.querySelector('.organize')!.dispatchEvent(drop));
    expect(drop.defaultPrevented).toBe(true);
    await waitFor(() => expect(imports).toHaveLength(1));
  });

  it('turns Add PDFs off, with the reason, when the document refuses any import', () => {
    setup(2, true, 'Adding pages rewrites the whole file, which needs both passwords.');
    const label = screen.getByText('Add PDFs');
    expect(label.getAttribute('aria-disabled')).toBe('true');
    expect(label.getAttribute('title')).toBe('Adding pages rewrites the whole file, which needs both passwords.');
  });

  it('shows one card per document in the Documents view, named after its file', async () => {
    const { blocks } = setup(2);
    await addPdfs([pdf('b.pdf', 3)]);
    fireEvent.click(button('Documents'));
    expect(blocks()).toEqual(['a.pdf, 2 pages', 'b.pdf, 3 pages']);
    // The added pages stay selected: as the block that holds them.
    expect(screen.getByText('Document 2 of 2 selected (3 pages)')).toBeTruthy();
    expect(screen.getByRole('listbox', { name: 'Documents in a.pdf' })).toBeTruthy();
  });

  it('moves, rotates and deletes a whole block as one page edit each', async () => {
    const { edits, blocks, order, grid, selected } = setup(2);
    await addPdfs([pdf('b.pdf', 3)]);
    fireEvent.click(button('Documents'));

    await act(async () => fireEvent.keyDown(grid(), { key: 'ArrowLeft', ctrlKey: true, shiftKey: true }));
    expect(edits).toEqual([{ kind: 'move', from: 2, count: 3, to: 0 }]);
    expect(blocks()).toEqual(['b.pdf, 3 pages', 'a.pdf, 2 pages']);
    expect(selected()).toEqual([0]);

    await act(async () => fireEvent.click(button('Rotate right')));
    expect(edits[1]).toEqual({ kind: 'rotate', from: 0, count: 3, degrees: 90 });

    await act(async () => fireEvent.click(button('Delete')));
    expect(edits[2]).toEqual({ kind: 'remove', from: 0, count: 3 });
    expect(blocks()).toEqual(['a.pdf, 2 pages']);
    fireEvent.click(button('Pages'));
    expect(order()).toEqual([0, 1]);
  });

  it('drags a block in front of another', async () => {
    const { edits, blocks, cards, grid } = setup(1);
    await addPdfs([pdf('b.pdf', 2)]);
    fireEvent.click(button('Documents'));
    const step = CARD.width + GRID_GAP;
    const middleOf = (column: number) => GRID_PADDING + column * step + CARD.width / 2;
    const y = GRID_PADDING + CARD.height / 2;
    fireEvent.pointerDown(cards()[1]!, { pointerId: 1, button: 0, clientX: middleOf(1), clientY: y });
    fireEvent.pointerMove(grid(), { pointerId: 1, clientX: middleOf(0) - 10, clientY: y });
    await act(async () => fireEvent.pointerUp(grid(), { pointerId: 1, clientX: middleOf(0) - 10, clientY: y }));
    expect(edits).toEqual([{ kind: 'move', from: 1, count: 2, to: 0 }]);
    expect(blocks()).toEqual(['b.pdf, 2 pages', 'a.pdf, 1 page']);
  });

  it('keeps the pages an edit selected when the view is switched while it is in flight', async () => {
    const { blocks, grid, holdEdits } = setup(2);
    await addPdfs([pdf('b.pdf', 3)]);
    // Pages view, pages 3–5 (the added block) selected: move them one place earlier.
    const release = holdEdits();
    await act(async () => fireEvent.keyDown(grid(), { key: 'ArrowLeft', ctrlKey: true, shiftKey: true }));
    fireEvent.click(button('Documents'));
    await act(async () => release());
    // a.pdf now sits on both sides of b.pdf: the moved pages are the middle block, and only it.
    expect(blocks()).toEqual(['a.pdf, 1 page', 'b.pdf, 3 pages', 'a.pdf, 1 page']);
    expect(screen.getByText('Document 2 of 3 selected (3 pages)')).toBeTruthy();
  });

  it('keeps a rotated block selected as its pages when switched to the Pages view mid-edit', async () => {
    const { selected, holdEdits } = setup(2);
    await addPdfs([pdf('b.pdf', 3)]);
    fireEvent.click(button('Documents'));
    const release = holdEdits();
    await act(async () => fireEvent.click(button('Rotate right')));
    fireEvent.click(button('Pages'));
    await act(async () => release());
    expect(selected()).toEqual([2, 3, 4]);
    expect(screen.getByText('Pages 3–5 of 5 selected')).toBeTruthy();
  });

  it('switches a selection spanning blocks to every block that holds it', async () => {
    const { cards } = setup(2);
    await addPdfs([pdf('b.pdf', 3)]);
    // Pages 2–3: the last of a.pdf and the first of b.pdf.
    fireEvent.click(cards()[1]!);
    fireEvent.click(cards()[2]!, { shiftKey: true });
    expect(screen.getByText('Pages 2–3 of 5 selected')).toBeTruthy();
    fireEvent.click(button('Documents'));
    expect(screen.getByText('Documents 1–2 of 2 selected (5 pages)')).toBeTruthy();
  });

  it('adds a pick that finishes while an edit is in flight once that edit lands, after the pages it left', async () => {
    const { imports, order, holdEdits } = setup(3);
    const release = holdEdits();
    await act(async () => fireEvent.click(button('Delete')));
    await addPdfs([pdf('b.pdf', 2)]);
    expect(imports).toEqual([]);
    await act(async () => release());
    await waitFor(() => expect(imports).toHaveLength(1));
    expect(imports).toEqual([[[1], 2]]);
    expect(order()).toEqual([1, 2, 100, 101]);
    expect(screen.getByText('Added 2 pages from b.pdf.')).toBeTruthy();
  });

  it('is blocked when the document withholds the assembly permission', async () => {
    const { edits, order, cards, grid } = setup(3, false);
    expect(screen.getByText(/does not allow its pages to be/)).toBeTruthy();
    for (const name of ['Rotate left', 'Rotate right', 'Delete', 'Move earlier', 'Move later']) {
      expect(button(name).getAttribute('aria-disabled')).toBe('true');
    }
    fireEvent.click(button('Delete'));
    await act(async () => fireEvent.keyDown(grid(), { key: 'Delete' }));
    fireEvent.pointerDown(cards()[0]!, { pointerId: 1, button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(grid(), { pointerId: 1, clientX: 400, clientY: 10 });
    await act(async () => fireEvent.pointerUp(grid(), { pointerId: 1, clientX: 400, clientY: 10 }));
    expect(edits).toEqual([]);
    expect(order()).toEqual([0, 1, 2]);
    // Looking around still works.
    fireEvent.keyDown(grid(), { key: 'ArrowRight' });
  });
});
