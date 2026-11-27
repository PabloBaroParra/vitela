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
  readonly pageIds?: readonly number[];
  readonly rotations?: readonly number[];
  readonly paused?: boolean;
}

const UPRIGHT: readonly number[] = [];

function mode(engine: Engine, initial: Props) {
  return renderHook(
    ({ active = true, visible, paintRevision = 0, baseGeneration = 0, pageIds = IDS, rotations = UPRIGHT, paused = false }: Props) =>
      useContentMode({ engine, handle: 1, active, pageIds, rotations, paused, paintRevision, baseGeneration, visible }),
    { initialProps: initial },
  );
}

/** An engine whose reads answer only when the test says so, in any order. */
function deferredEngine() {
  const reads: number[] = [];
  const answers: (() => void)[] = [];
  const engine: Engine = fakeEngine({
    readPageContent: (_document, pageIndex) =>
      new Promise((resolve) => {
        reads.push(pageIndex);
        answers.push(() => resolve(contentOf(pageIndex)));
      }),
  });
  /** Answers the read asked `n`-th (0-based). */
  const answer = async (n: number) => {
    await act(async () => answers[n]!());
  };
  return { engine, reads, answer };
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

  it('parses a page again when it is turned under the same id and revision', async () => {
    const { engine, reads } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 0, last: 0 } });
    await waitFor(() => expect(result.current.stateAt(1)?.status).toBe('ready'));
    expect(reads).toEqual([0, 1]);
    // An undo of a rotation made in the mode: page 0 turns, nothing else.
    rerender({ visible: { first: 0, last: 0 }, rotations: [90] });
    expect(result.current.stateAt(0)?.status).not.toBe('ready');
    await waitFor(() => expect(result.current.stateAt(0)?.status).toBe('ready'));
    expect(reads).toEqual([0, 1, 0]);
  });

  it('asks nothing while an edit is pending, then reads the pages as they are', async () => {
    const { engine, reads } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 0, last: 0 }, paused: true });
    await act(async () => {});
    expect(reads).toEqual([]);
    rerender({ visible: { first: 0, last: 0 }, paused: false });
    await waitFor(() => expect(result.current.stateAt(1)?.status).toBe('ready'));
    expect(reads).toEqual([0, 1]);
  });

  it('while an edit is pending, no page is ready and a press picks nothing from a parse it may be about to replace', async () => {
    const { engine } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 0, last: 0 } });
    await waitFor(() => expect(result.current.stateAt(0)?.status).toBe('ready'));
    // An undo starts: the parse held for page 1 may name what it is about to take back.
    rerender({ visible: { first: 0, last: 0 }, paused: true });
    expect(result.current.stateAt(0)).toBeUndefined();
    act(() => result.current.select(0, { x: 80, y: 78 }));
    act(() => result.current.hover(0, { x: 80, y: 78 }));
    expect(result.current.selected).toBeNull();
    expect(result.current.hovered).toBeNull();
    // It lands without changing the page: the same parse is ready and found again.
    rerender({ visible: { first: 0, last: 0 }, paused: false });
    expect(result.current.stateAt(0)?.status).toBe('ready');
    act(() => result.current.select(0, { x: 80, y: 78 }));
    expect(result.current.selected?.item).toMatchObject({ kind: 'run', text: 'Page 1' });
  });

  it("drops an answer whose position holds another page by the time it lands, and reads that page where it is", async () => {
    const { engine, reads, answer } = deferredEngine();
    const { result, rerender } = mode(engine, { visible: { first: 0, last: 0 }, pageIds: [10, 11] });
    await act(async () => {});
    expect(reads).toEqual([0, 1]);
    // The two pages swap places (an Organize move landing) before either read answers.
    rerender({ visible: { first: 0, last: 0 }, pageIds: [11, 10] });
    await answer(0);
    await answer(1);
    // Both answers were for positions that now hold the other page: read again.
    await act(async () => {});
    expect(reads).toHaveLength(4);
    expect([...reads.slice(2)].sort()).toEqual([0, 1]);
    await answer(2);
    await answer(3);
    expect(result.current.stateAt(0)).toMatchObject({ status: 'ready', content: { runs: [{ text: 'Page 1' }] } });
  });

  it('reads a few pages at a time, never one already in flight, and skips the pages a fast scroll passed', async () => {
    const { engine, reads, answer } = deferredEngine();
    const { rerender } = mode(engine, { visible: { first: 0, last: 0 } });
    await act(async () => {});
    expect(reads).toEqual([0, 1]);
    // Scrolled far and back before either read answered: nothing asked twice.
    rerender({ visible: { first: 500, last: 500 } });
    rerender({ visible: { first: 0, last: 0 } });
    await act(async () => {});
    expect(reads).toEqual([0, 1]);
    // Then on, past pages 100 and 200: only where the scroll ends is read.
    rerender({ visible: { first: 100, last: 100 } });
    rerender({ visible: { first: 200, last: 200 } });
    await answer(0);
    await answer(1);
    expect(reads).toEqual([0, 1, 199, 200]);
    await answer(2);
    expect(reads).toEqual([0, 1, 199, 200, 201]);
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

  it('selects the edited run again once its page is read after the edit', async () => {
    const { engine } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 2 } });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 80, y: 80 }));
    const run = result.current.selected!.item;

    // The edit is asked for, then lands: the page repaints and is read again.
    act(() => result.current.follow(2, run.rect, 'Page 3'));
    rerender({ visible: { first: 2, last: 2 }, paused: true });
    expect(result.current.selected?.item).toBe(run);
    rerender({ visible: { first: 2, last: 2 }, paused: false, paintRevision: 1 });
    expect(result.current.selected).toBeNull();
    await waitFor(() => expect(result.current.selected?.item).toMatchObject({ kind: 'run', text: 'Page 3' }));
    // Found in the read after the edit (paint revision 1).
    expect(result.current.selected!.key).toMatch(/\/1\/0$/);
  });

  it('forgets what to follow when the edit changed nothing, and lets the selection go on a later repaint', async () => {
    const { engine } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 2 } });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 80, y: 80 }));
    act(() => result.current.follow(2, result.current.selected!.item.rect, 'Page 3'));
    rerender({ visible: { first: 2, last: 2 }, paused: true });
    rerender({ visible: { first: 2, last: 2 }, paused: false });
    expect(result.current.selected?.item.kind).toBe('run');
    // An undo elsewhere: nothing follows.
    rerender({ visible: { first: 2, last: 2 }, paintRevision: 7 });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    expect(result.current.selected).toBeNull();
  });

  it('lets a selection made after an edit was asked for win over selecting the edited run again', async () => {
    // Page 3 holds two runs: "Page 3" and "Second" below it.
    const engine: Engine = fakeEngine({
      readPageContent: async (_document, pageIndex) => {
        const content = contentOf(pageIndex);
        const second = { ...content.runs[0]!, id: 1, rect: { left: 72, top: 120, width: 100, height: 14 }, text: 'Second' };
        return { ...content, runs: [...content.runs, second] };
      },
    });
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 3 } });
    await waitFor(() => expect(result.current.stateAt(3)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 80, y: 80 }));
    const edited = result.current.selected!.item;

    // A press on another run of the same page commits the edit, then selects it.
    act(() => {
      result.current.follow(2, edited.rect, 'Edited');
      result.current.select(2, { x: 80, y: 125 });
    });
    rerender({ visible: { first: 2, last: 3 }, paused: true });
    rerender({ visible: { first: 2, last: 3 }, paused: false, paintRevision: 1 });
    await waitFor(() => expect(result.current.selected?.item).toMatchObject({ kind: 'run', text: 'Second' }));
    expect(result.current.following).toBe(false);

    // A press on another page: that run is selected again once the pages are read after the edit.
    act(() => {
      result.current.follow(2, result.current.selected!.item.rect, 'Second!');
      result.current.select(3, { x: 80, y: 80 });
    });
    rerender({ visible: { first: 2, last: 3 }, paused: true, paintRevision: 1 });
    rerender({ visible: { first: 2, last: 3 }, paused: false, paintRevision: 2 });
    await waitFor(() => expect(result.current.selected).toMatchObject({ pageIndex: 3, item: { text: 'Page 4' } }));
    expect(result.current.selected!.key).toMatch(/\/2\/0$/);
    expect(result.current.following).toBe(false);

    // A press on nothing: nothing is selected again.
    act(() => {
      result.current.follow(3, result.current.selected!.item.rect, 'Page 4!');
      result.current.select(3, { x: 500, y: 700 });
    });
    expect(result.current.following).toBe(false);
    expect(result.current.selected).toBeNull();
  });

  it('selects the edited image again by its new box once its page is read after the edit', async () => {
    let moved = false;
    const engine: Engine = fakeEngine({
      readPageContent: async (_document, pageIndex) => {
        const content = contentOf(pageIndex);
        const [image] = content.images;
        return moved ? { ...content, images: [{ ...image!, id: 5, rect: { ...image!.rect, left: 92 } }] } : content;
      },
    });
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 2 } });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 100, y: 350 }));
    const image = result.current.selected!.item;
    expect(image.kind).toBe('image');

    act(() => result.current.followImage(2, { ...image.rect, left: 92 }));
    expect(result.current.following).toBe(true);
    expect(result.current.followingKind).toBe('image');
    moved = true;
    rerender({ visible: { first: 2, last: 2 }, paused: true });
    rerender({ visible: { first: 2, last: 2 }, paused: false, paintRevision: 1 });
    await waitFor(() => expect(result.current.selected?.item).toMatchObject({ kind: 'image', id: 5 }));
    expect(result.current.following).toBe(false);

    // A removed image selects nothing.
    act(() => result.current.followImage(2, null));
    rerender({ visible: { first: 2, last: 2 }, paused: false, paintRevision: 2 });
    await waitFor(() => expect(result.current.following).toBe(false));
    expect(result.current.selected).toBeNull();
  });

  it('follows an image pressed while a run edit lands', async () => {
    const { engine } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 2 } });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 80, y: 80 }));
    act(() => {
      result.current.follow(2, result.current.selected!.item.rect, 'Page 3');
      result.current.select(2, { x: 100, y: 350 });
    });
    expect(result.current.followingKind).toBe('image');
    rerender({ visible: { first: 2, last: 2 }, paused: false, paintRevision: 1 });
    await waitFor(() => expect(result.current.selected?.item.kind).toBe('image'));
    expect(result.current.selected!.key).toMatch(/\/1\/0$/);
  });

  it('drops the follow of a run whose page scrolled out of the parse window, so edits are not held up', async () => {
    const { engine, reads } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 2 } });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 80, y: 80 }));
    const run = result.current.selected!.item;

    // The editor's page scrolled far away and the commit that caused asks to follow it.
    rerender({ visible: { first: 40, last: 40 } });
    act(() => result.current.follow(2, run.rect, 'Page 3!'));
    rerender({ visible: { first: 40, last: 40 }, paused: true });
    rerender({ visible: { first: 40, last: 40 }, paused: false, paintRevision: 1 });
    // Page 3 is never read again (it is not wanted), and nothing waits for it.
    await waitFor(() => expect(result.current.stateAt(40)?.status).toBe('ready'));
    expect(result.current.following).toBe(false);
    expect(reads.filter((page) => page === 2)).toHaveLength(1);

    // Coming back reads it, and selects nothing the user did not pick.
    rerender({ visible: { first: 2, last: 2 }, paintRevision: 1 });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    expect(result.current.selected).toBeNull();
  });

  it('follows nothing for a removed run', async () => {
    const { engine } = engineWith();
    const { result, rerender } = mode(engine, { visible: { first: 2, last: 2 } });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    act(() => result.current.select(2, { x: 80, y: 80 }));
    act(() => result.current.follow(2, result.current.selected!.item.rect, null));
    rerender({ visible: { first: 2, last: 2 }, paintRevision: 1 });
    await waitFor(() => expect(result.current.stateAt(2)?.status).toBe('ready'));
    expect(result.current.selected).toBeNull();
  });

  it('says which page could not be read', async () => {
    const engine = fakeEngine({
      readPageContent: async () => {
        throw Object.assign(new Error('text extraction is not permitted'), { name: 'TextExtractionDenied' });
      },
    });
    const { result } = renderHook(() =>
      useContentMode({
        engine,
        handle: 1,
        active: true,
        pageIds: [5],
        rotations: [0],
        paused: false,
        paintRevision: 0,
        baseGeneration: 0,
        visible: { first: 0, last: 0 },
      }),
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

  it('offers Insert image, pressed while an insert waits for a click', () => {
    let asked = 0;
    const { rerender } = render(<ContentStatus status="x" onDone={() => {}} onInsertImage={() => asked++} />);
    const insert = screen.getByRole('button', { name: 'Insert image' });
    expect(insert.getAttribute('aria-pressed')).toBe('false');
    fireEvent.click(insert);
    expect(asked).toBe(1);
    rerender(<ContentStatus status="x" onDone={() => {}} onInsertImage={() => asked++} inserting />);
    expect(screen.getByRole('button', { name: 'Insert image' }).getAttribute('aria-pressed')).toBe('true');
  });
});
