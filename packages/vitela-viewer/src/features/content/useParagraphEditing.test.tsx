import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ContentRun, Edit, PageContent, TextBlock, TextRunStyle } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { Session } from '../annotations/useAnnotationEdits';
import { TextBlockEditor } from './TextBlockEditor';
import type { ContentMode } from './useContentMode';
import { useParagraphEditing, type ParagraphTarget } from './useParagraphEditing';
import type { Drafted } from './useDraftEditor';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const run = (id: number, top: number, text: string, editable = true): ContentRun => ({
  kind: 'run',
  id,
  pageIndex: 0,
  rect: { left: 72, top, width: 120, height: 12 },
  fontName: 'F1',
  fontKind: 'standard14',
  text,
  editable,
});
const first = run(3, 130, 'First line');
const second = run(4, 144, 'Second line');
const lone = run(7, 300, 'Alone');
const paragraph: TextBlock = {
  runIds: [3, 4],
  rect: { left: 72, top: 130, width: 120, height: 26 },
  text: 'First line\nSecond line',
  lines: [
    { runIds: [3], rect: first.rect, text: first.text },
    { runIds: [4], rect: second.rect, text: second.text },
  ],
  leadingPt: 14,
  alignment: 'left',
};
const single: TextBlock = { runIds: [7], rect: lone.rect, text: 'Alone', lines: [{ runIds: [7], rect: lone.rect, text: 'Alone' }], leadingPt: null, alignment: 'left' };
const STYLE: TextRunStyle = { baseFont: 'Helvetica', fontSizePt: 12, color: { r: 10, g: 20, b: 30 }, charSpacingPt: 0, horizontalScale: 1, bold: false, italic: false, renderMode: 0 };
const info = { pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], pageIds: [9] };
const session: Session = { info, annotations: [] };

function setup({ selected = first as ContentRun | null, runs = [first, second, lone] as ContentRun[] } = {}) {
  const applied: (Edit | null)[] = [];
  const follows: unknown[][] = [];
  const notices: string[] = [];
  const reads: unknown[][] = [];
  const engine = fakeEngine({
    readParagraphBlocks: async (...args) => {
      reads.push(args);
      return [paragraph, single];
    },
    textRunStyle: async () => STYLE,
  });
  const page: PageContent = { runs, images: [], paths: [] };
  const content = (item: ContentRun | null, key: string) =>
    ({
      active: true,
      selected: item && { pageIndex: item.pageIndex, key, item },
      following: false,
      stateAt: () => ({ status: 'ready', content: page }),
      follow: (...args: unknown[]) => void follows.push(args),
    }) as unknown as ContentMode;
  const queue = {
    pending: false,
    apply: async (build: (session: Session) => Edit | null) => void applied.push(build(session)),
    notify: (message: string) => void notices.push(message),
  };
  const focusPages = vi.fn();
  const hook = renderHook(
    ({ item, running, key = '9/0/0/0' }: { item: ContentRun | null; running: boolean; key?: string }) =>
      useParagraphEditing({ engine, handle: 1, info, content: content(item, key), queue: { ...queue, pending: running }, focusPages }),
    { initialProps: { item: selected, running: false } as { item: ContentRun | null; running: boolean; key?: string } },
  );
  return { hook, applied, follows, notices, reads, focusPages };
}

describe('useParagraphEditing', () => {
  it('reads the paragraphs of the selected run’s page once per read, and finds its paragraph', async () => {
    const { hook, reads } = setup();
    await waitFor(() => expect(hook.result.current.paragraph).toBe(paragraph));
    expect(reads).toEqual([[1, 0]]);
    hook.rerender({ item: second, running: false });
    expect(hook.result.current.paragraph).toBe(paragraph);
    expect(reads).toHaveLength(1);
    hook.rerender({ item: second, running: false, key: '9/0/1/0' });
    await waitFor(() => expect(reads).toHaveLength(2));
  });

  it('offers nothing for a run on one line, or with nothing selected', async () => {
    const { hook, reads } = setup({ selected: lone });
    await waitFor(() => expect(reads).toHaveLength(1));
    await act(async () => {});
    expect(hook.result.current.paragraph).toBeNull();
    hook.rerender({ item: null, running: false });
    expect(hook.result.current.paragraph).toBeNull();
  });

  it('reads nothing while a step runs', async () => {
    const { hook, reads } = setup();
    hook.rerender({ item: first, running: true, key: '9/0/2/0' });
    await act(async () => {});
    expect(reads).toHaveLength(1);
    expect(hook.result.current.paragraph).toBeNull();
  });

  it('opens the paragraph’s text in an editor in its first run’s style', async () => {
    const { hook } = setup();
    await waitFor(() => expect(hook.result.current.paragraph).toBe(paragraph));
    act(() => hook.result.current.open());
    expect(hook.result.current.editor).toMatchObject({ block: paragraph, pageIndex: 0, draft: 'First line\nSecond line' });
    await waitFor(() => expect(hook.result.current.editor?.style).toEqual(STYLE));
  });

  it('opens on Shift+Enter at the pages, and leaves every other key to the run', async () => {
    const { hook } = setup();
    await waitFor(() => expect(hook.result.current.paragraph).toBe(paragraph));
    expect(hook.result.current.onKey('Enter', false)).toBe(false);
    let used = false;
    act(() => void (used = hook.result.current.onKey('Enter', true)));
    expect(used).toBe(true);
    expect(hook.result.current.editor).not.toBeNull();
  });

  it('records the whole paragraph as one retype and follows its first line', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { hook, applied, follows } = setup();
    await waitFor(() => expect(hook.result.current.paragraph).toBe(paragraph));
    act(() => hook.result.current.open());
    act(() => hook.result.current.typing('New first\nSecond line\nThird'));
    act(() => hook.result.current.commit(true));
    expect(applied).toEqual([{ kind: 'retypeTextBlock', page: 0, runIds: [3, 4], text: 'New first\nSecond line\nThird' }]);
    expect(follows).toEqual([[0, first.rect, 'New first']]);
    act(() => void vi.runAllTimers());
    expect(hook.result.current.editor).toBeNull();
  });

  it('records nothing for the same text, and follows nothing for an emptied paragraph', async () => {
    const { hook, applied, follows } = setup();
    await waitFor(() => expect(hook.result.current.paragraph).toBe(paragraph));
    act(() => hook.result.current.open());
    act(() => hook.result.current.commit(false));
    expect(applied).toEqual([]);
    await waitFor(() => expect(hook.result.current.editor).toBeNull());

    act(() => hook.result.current.open());
    act(() => hook.result.current.typing(''));
    act(() => hook.result.current.commit(false));
    expect(applied).toEqual([{ kind: 'retypeTextBlock', page: 0, runIds: [3, 4], text: '' }]);
    expect(follows).toEqual([[0, first.rect, null]]);
  });

  it('says so, and opens nothing, for a paragraph with a run whose text did not decode', async () => {
    const broken = run(4, 144, 'Second l?ne', false);
    const { hook, notices } = setup({ runs: [first, broken, lone] });
    await waitFor(() => expect(hook.result.current.paragraph).toBe(paragraph));
    act(() => hook.result.current.open());
    expect(hook.result.current.editor).toBeNull();
    expect(notices).toEqual(['This text cannot be retyped: its font does not say what all of its characters are.']);
  });
});

describe('TextBlockEditor', () => {
  const editor: Drafted<ParagraphTarget> = { block: paragraph, pageIndex: 0, pageKey: '9/0', style: STYLE, draft: paragraph.text };

  it('lays a multi-line field over the paragraph in its size, colour and leading', () => {
    render(<TextBlockEditor editor={editor} scale={2} rotationDegrees={0} onCommit={() => {}} onCancel={() => {}} onType={() => {}} onAttach={() => {}} onDetach={() => {}} />);
    const field = screen.getByRole('textbox', { name: 'Edit paragraph “First line…”' });
    expect(field.tagName).toBe('TEXTAREA');
    expect(field.style.left).toBe('144px');
    expect(field.style.top).toBe('260px');
    expect(field.style.width).toBe('240px');
    expect(field.style.fontSize).toBe('24px');
    expect(field.style.lineHeight).toBe('28px');
    expect(field.style.color).toBe('rgb(10, 20, 30)');
    expect((field as HTMLTextAreaElement).value).toBe('First line\nSecond line');
    expect(document.activeElement).toBe(field);
  });

  it('keeps Enter a line break, commits on Ctrl+Enter or leaving it, cancels on Escape', () => {
    const onCommit = vi.fn();
    const onCancel = vi.fn();
    const pages = vi.fn();
    render(
      <div onKeyDown={pages}>
        <TextBlockEditor editor={editor} scale={1} rotationDegrees={0} onCommit={onCommit} onCancel={onCancel} onType={() => {}} onAttach={() => {}} onDetach={() => {}} />
      </div>,
    );
    const field = screen.getByRole('textbox');
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(onCommit).not.toHaveBeenCalled();
    expect(pages).not.toHaveBeenCalled();
    fireEvent.keyDown(field, { key: 'Enter', ctrlKey: true });
    expect(onCommit).toHaveBeenCalledWith(true);
    fireEvent.keyDown(field, { key: 'Enter', metaKey: true });
    expect(onCommit).toHaveBeenCalledTimes(2);
    fireEvent.keyDown(field, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalled();
    fireEvent.blur(field);
    expect(onCommit).toHaveBeenLastCalledWith(false);
  });
});
