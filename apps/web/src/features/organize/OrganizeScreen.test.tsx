import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useState } from 'react';
import type { DocumentInfo, EditedDocument, Engine, PageEdit, RenderedPage } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { MarkDirtyContext } from '../save/MarkDirtyContext';
import { CARD, GRID_GAP, GRID_PADDING } from './gridLayout';
import { OrganizeScreen } from './OrganizeScreen';

/** Room for exactly four cards a row. */
const VIEWPORT = { width: 2 * GRID_PADDING + 4 * CARD.width + 3 * GRID_GAP, height: 600 };
const bitmap: RenderedPage = { width: 1, height: 1, rgba: new Uint8ClampedArray(4) };

function info(ids: readonly number[], turns: readonly number[] = ids.map(() => 0), allowed = true): DocumentInfo {
  return {
    pageCount: ids.length,
    pages: turns.map((rotationDegrees) => ({ widthPt: 612, heightPt: 792, rotationDegrees })),
    pageIds: ids,
    textExtractionAllowed: true,
    documentAssemblyAllowed: allowed,
  };
}

/** An engine over a page list, applying edits the way the core does. */
function pagesEngine(count: number, allowed = true) {
  let pages = Array.from({ length: count }, (_, id) => ({ id, turn: 0 }));
  const undo: (typeof pages)[] = [];
  const redo: (typeof pages)[] = [];
  const edits: PageEdit[] = [];
  const answer = (): EditedDocument => ({
    info: info(
      pages.map((p) => p.id),
      pages.map((p) => p.turn),
      allowed,
    ),
    canUndo: undo.length > 0,
    canRedo: redo.length > 0,
  });
  const engine: Engine = fakeEngine({
    renderPage: async () => bitmap,
    applyEdit: async (_handle, edit) => {
      edits.push(edit);
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
      undo.push(pages);
      redo.length = 0;
      pages = next;
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
  });
  return { engine, edits, opened: answer().info };
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
      />
    </MarkDirtyContext.Provider>
  );
}

function setup(count: number, allowed = true) {
  const fake = pagesEngine(count, allowed);
  const markDirty = vi.fn();
  const view = render(<Harness engine={fake.engine} initial={fake.opened} markDirty={markDirty} />);
  const grid = () => screen.getByRole('listbox');
  const cards = () => [...view.container.querySelectorAll<HTMLElement>('.page-card')];
  /** Page ids in grid order. */
  const order = () => cards().map((card) => Number(card.dataset.pageId));
  const selected = () => cards().filter((card) => card.getAttribute('aria-selected') === 'true').map((c) => Number(c.dataset.index));
  return { ...fake, ...view, markDirty, grid, cards, order, selected };
}

const button = (name: string) => screen.getByRole('button', { name });

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
