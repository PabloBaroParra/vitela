import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { Engine, PageContent } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { ContentStatus } from './ContentStatus';
import { CONTENT_HINT } from './contentModel';
import { useContentGate, useContentMode } from './useContentMode';

afterEach(cleanup);

/** One run "Page N" at the top-left of every page, and an image below it. */
const contentOf = (pageIndex: number): PageContent => ({
  runs: [
    {
      kind: 'run',
      id: 0,
      pageIndex,
      rect: { left: 72, top: 72, width: 100, height: 14 },
      fontName: 'F1',
      fontKind: 'standard14',
      text: `Page ${pageIndex + 1}`,
      editable: true,
    },
  ],
  images: [{ kind: 'image', id: 0, pageIndex, rect: { left: 72, top: 300, width: 200, height: 100 }, inline: false }],
  paths: [],
});

function engineWith(refusal: string | null = null) {
  const reads: number[] = [];
  const asked: number[] = [];
  const engine: Engine = fakeEngine({
    contentEditingRefusal: async (document) => (asked.push(document), refusal),
    readPageContent: async (_document, pageIndex) => {
      reads.push(pageIndex);
      return contentOf(pageIndex);
    },
  });
  return { engine, reads, asked };
}

const IDS = Array.from({ length: 1619 }, (_, index) => 1000 + index);

interface Props {
  readonly active?: boolean;
  readonly visible: { first: number; last: number } | null;
  readonly paintRevision?: number;
  readonly baseGeneration?: number;
}

function mode(engine: Engine, initial: Props) {
  return renderHook(
    ({ active = true, visible, paintRevision = 0, baseGeneration = 0 }: Props) =>
      useContentMode({ engine, handle: 1, active, pageIds: IDS, paintRevision, baseGeneration, visible }),
    { initialProps: initial },
  );
}

describe('useContentGate', () => {
  it("asks once per document, and words a refusal as the core's sentence", async () => {
    const { engine, asked } = engineWith('content editing is not permitted');
    const { result, rerender } = renderHook(({ handle }: { handle: number | null }) => useContentGate(engine, handle), {
      initialProps: { handle: null as number | null },
    });
    expect(result.current).toEqual({ status: 'checking' });
    expect(asked).toEqual([]);
    rerender({ handle: 3 });
    expect(result.current).toEqual({ status: 'checking' });
    await waitFor(() => expect(result.current).toEqual({ status: 'refused', reason: 'Content editing is not permitted.' }));
    rerender({ handle: 3 });
    expect(asked).toEqual([3]);
  });

  it('allows a document the core allows, and refuses one it could not ask about', async () => {
    const allowed = engineWith(null);
    const { result } = renderHook(() => useContentGate(allowed.engine, 1));
    await waitFor(() => expect(result.current).toEqual({ status: 'allowed' }));

    const broken = fakeEngine({
      contentEditingRefusal: async () => {
        throw new Error('no open document with id 1');
      },
    });
    const failed = renderHook(() => useContentGate(broken, 1));
    await waitFor(() => expect(failed.result.current).toEqual({ status: 'refused', reason: 'No open document with id 1.' }));
  });
});

describe('useContentMode', () => {
  it('parses nothing while off', async () => {
    const { engine, reads } = engineWith();
    const { result } = mode(engine, { active: false, visible: { first: 0, last: 0 } });
    expect(result.current.status).toBeNull();
    await act(async () => {});
    expect(reads).toEqual([]);
  });

  it('parses only the pages in view and one on each side of a long document', async () => {
    const { engine, reads } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 800, last: 801 } });
    await waitFor(() => expect(result.current.stateAt(801)?.status).toBe('ready'));
    expect([...reads].sort((a, b) => a - b)).toEqual([799, 800, 801, 802]);
    expect(result.current.stateAt(0)).toBeUndefined();
    expect(result.current.status).toBe(CONTENT_HINT);

    // Scrolling on parses the newly near pages and lets the far ones go.
    rerender({ visible: { first: 802, last: 802 } });
    await waitFor(() => expect(result.current.stateAt(803)?.status).toBe('ready'));
    expect(reads.slice(4)).toEqual([803]);
    expect(result.current.stateAt(799)).toBeUndefined();
    expect(result.current.stateAt(801)?.status).toBe('ready');
  });

  it('parses a page again when an edit repaints it or the core swaps its base', async () => {
    const { engine, reads } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 0, last: 0 } });
    await waitFor(() => expect(result.current.stateAt(1)?.status).toBe('ready'));
    expect(reads).toEqual([0, 1]);

    rerender({ visible: { first: 0, last: 0 }, paintRevision: 1 });
    await waitFor(() => expect(reads).toHaveLength(4));
    rerender({ visible: { first: 0, last: 0 }, paintRevision: 1, baseGeneration: 1 });
    await waitFor(() => expect(reads).toHaveLength(6));
    rerender({ visible: { first: 0, last: 0 }, paintRevision: 1, baseGeneration: 1 });
    expect(reads).toHaveLength(6);
  });

  it('outlines what is under the pointer, selects on a press, and lets go on clear, an edit or leaving', async () => {
    const { engine } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 2 } });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));

    act(() => result.current.hover(2, { x: 80, y: 80 }));
    expect(result.current.hovered).toMatchObject({ pageIndex: 2, item: { kind: 'run', text: 'Page 3' } });
    act(() => result.current.hover(2, { x: 100, y: 350 }));
    expect(result.current.hovered?.item.kind).toBe('image');
    act(() => result.current.hover(2, null));
    expect(result.current.hovered).toBeNull();

    act(() => result.current.select(2, { x: 80, y: 80 }));
    expect(result.current.status).toBe('Text “Page 3” selected on page 3.');
    act(() => result.current.clear());
    expect(result.current.status).toBe(CONTENT_HINT);

    act(() => result.current.select(2, { x: 100, y: 350 }));
    expect(result.current.status).toBe('Image selected on page 3.');
    // A press on nothing lets it go.
    act(() => result.current.select(2, { x: 500, y: 700 }));
    expect(result.current.selected).toBeNull();

    // An edit that repaints the page makes the selection name something else.
    act(() => result.current.select(2, { x: 80, y: 80 }));
    rerender({ visible: { first: 2, last: 2 }, paintRevision: 4 });
    expect(result.current.selected).toBeNull();

    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 80, y: 80 }));
    rerender({ active: false, visible: { first: 2, last: 2 }, paintRevision: 4 });
    expect(result.current.selected).toBeNull();
    expect(result.current.status).toBeNull();
    // Back on, nothing is selected from before.
    rerender({ active: true, visible: { first: 2, last: 2 }, paintRevision: 4 });
    expect(result.current.selected).toBeNull();
  });

  it('says which page could not be read', async () => {
    const engine = fakeEngine({
      readPageContent: async () => {
        throw Object.assign(new Error('text extraction is not permitted'), { name: 'TextExtractionDenied' });
      },
    });
    const { result } = renderHook(() =>
      useContentMode({ engine, handle: 1, active: true, pageIds: [5], paintRevision: 0, baseGeneration: 0, visible: { first: 0, last: 0 } }),
    );
    await waitFor(() =>
      expect(result.current.status).toBe('Could not read the content of page 1: Text extraction is not permitted.'),
    );
  });
});

describe('ContentStatus', () => {
  it('shows nothing while the mode is off', () => {
    render(<ContentStatus status={null} onDone={() => {}} />);
    expect(screen.queryByRole('status')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Done' })).toBeNull();
  });

  it('says what is selected in a polite live region, with Done', () => {
    let done = 0;
    render(<ContentStatus status="Image selected on page 1." onDone={() => done++} />);
    const status = screen.getByRole('status');
    expect(status.textContent).toBe('Image selected on page 1.');
    expect(status.getAttribute('aria-live')).toBe('polite');
    fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    expect(done).toBe(1);
  });
});
