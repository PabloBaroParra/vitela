import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ContentRun, Edit, TextRunLookState } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { Session } from '../annotations/useAnnotationEdits';
import { TextRunEditor } from './TextRunEditor';
import { TextRunToolbar } from './TextRunToolbar';
import type { ContentMode } from './useContentMode';
import { useTextRunEditing, type OpenEditor } from './useTextRunEditing';

afterEach(cleanup);

const hello: ContentRun = {
  kind: 'run',
  id: 4,
  pageIndex: 0,
  rect: { left: 72, top: 80, width: 30, height: 12 },
  fontName: 'F1',
  fontKind: 'standard14',
  text: 'Hello',
  editable: true,
};
const PLAIN: TextRunLookState = {
  bold: false,
  italic: false,
  underline: false,
  strike: false,
  canRemoveBold: true,
  canRemoveItalic: true,
};
const info = { pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], pageIds: [9] };
const session: Session = { info, annotations: [] };

function setup({ run = hello, pending = false, look = PLAIN }: { run?: ContentRun; pending?: boolean; look?: TextRunLookState } = {}) {
  const applied: (Edit | null)[] = [];
  const follows: unknown[][] = [];
  const notices: string[] = [];
  const engine = fakeEngine({
    textRunLook: async () => look,
    textRunStyle: async () => null,
    textRunFontProgram: async () => null,
  });
  const content = {
    active: true,
    selected: { pageIndex: 0, key: '9/0/0/0', item: run },
    following: false,
    follow: (...args: unknown[]) => void follows.push(args),
  } as unknown as ContentMode;
  const queue = {
    pending,
    apply: async (build: (session: Session) => Edit | null) => void applied.push(build(session)),
    notify: (message: string) => void notices.push(message),
  };
  const focusPages = vi.fn();
  const hook = renderHook(
    ({ pending: running }: { pending: boolean }) =>
      useTextRunEditing({ engine, handle: 1, info, content, queue: { ...queue, pending: running }, focusPages }),
    { initialProps: { pending } },
  );
  return { hook, applied, follows, notices, focusPages };
}

describe('useTextRunEditing', () => {
  it('moves the selected run by an arrow and follows it to its new place', () => {
    const { hook, applied, follows } = setup();
    let used = false;
    act(() => void (used = hook.result.current.onKey('ArrowRight', true)));
    expect(used).toBe(true);
    expect(applied).toEqual([{ kind: 'moveTextRun', page: 0, runId: 4, dx: 10, dy: 0 }]);
    expect(follows).toEqual([[0, { ...hello.rect, left: 82 }, 'Hello']]);
    expect(hook.result.current.onKey('a', false)).toBe(false);
  });

  it('adds up the arrows pressed while a step runs into one move once it is done', () => {
    const { hook, applied } = setup({ pending: true });
    act(() => {
      hook.result.current.move(1, 0);
      hook.result.current.move(1, 0);
      hook.result.current.move(0, -1);
    });
    expect(applied).toEqual([]);
    hook.rerender({ pending: false });
    expect(applied).toEqual([{ kind: 'moveTextRun', page: 0, runId: 4, dx: 2, dy: -1 }]);
  });

  it('keeps the arrows pressed while the moved run is being found again, and sends them once it is', () => {
    const applied: (Edit | null)[] = [];
    const engine = fakeEngine({ textRunLook: async () => PLAIN });
    const contentOf = (selected: boolean) =>
      ({
        active: true,
        // Its page was edited and is read again: the old pick no longer holds.
        selected: selected ? { pageIndex: 0, key: '9/0/1/0', item: { ...hello, id: 7 } } : null,
        following: !selected,
        follow: () => {},
      }) as unknown as ContentMode;
    const queue = { pending: false, apply: async (build: (s: Session) => Edit | null) => void applied.push(build(session)), notify: () => {} };
    const hook = renderHook(
      ({ selected }: { selected: boolean }) =>
        useTextRunEditing({ engine, handle: 1, info, content: contentOf(selected), queue, focusPages: () => {} }),
      { initialProps: { selected: false } },
    );
    let used = false;
    act(() => void (used = hook.result.current.onKey('ArrowRight', true)));
    expect(used).toBe(true);
    // Other keys still need a run.
    expect(hook.result.current.onKey('Delete', false)).toBe(false);
    expect(applied).toEqual([]);
    hook.rerender({ selected: true });
    expect(applied).toEqual([{ kind: 'moveTextRun', page: 0, runId: 7, dx: 10, dy: 0 }]);
  });

  it('deletes on Delete and follows nothing', () => {
    const { hook, applied, follows } = setup();
    act(() => void hook.result.current.onKey('Delete', false));
    expect(applied).toEqual([{ kind: 'removeTextRun', page: 0, runId: 4 }]);
    expect(follows[0]?.[2]).toBeNull();
  });

  it('toggles bold from the look the core read', async () => {
    const { hook, applied } = setup();
    await waitFor(() => expect(hook.result.current.look).toEqual(PLAIN));
    act(() => hook.result.current.toggle('bold'));
    expect(applied).toEqual([
      { kind: 'restyleTextRuns', page: 0, runIds: [4], look: { bold: true, italic: false, underline: false, strike: false } },
    ]);
  });

  it('opens the editor on Enter and records a retype, a removal for no text, nothing for the same text', async () => {
    const { hook, applied, focusPages } = setup();
    act(() => void hook.result.current.onKey('Enter', false));
    expect(hook.result.current.editor?.run).toBe(hello);
    expect(hook.result.current.editor?.draft).toBe('Hello');
    act(() => hook.result.current.typing('Howdy'));
    expect(hook.result.current.editor?.draft).toBe('Howdy');
    act(() => hook.result.current.commit(true));
    expect(applied).toEqual([{ kind: 'retypeTextRun', page: 0, runId: 4, text: 'Howdy' }]);
    expect(focusPages).toHaveBeenCalled();
    // Closed a turn later, not inside the blur that may have committed it.
    expect(hook.result.current.editor).not.toBeNull();
    await waitFor(() => expect(hook.result.current.editor).toBeNull());

    act(() => hook.result.current.open());
    act(() => hook.result.current.commit(false));
    expect(applied).toHaveLength(1);
    await waitFor(() => expect(hook.result.current.editor).toBeNull());
    act(() => hook.result.current.open());
    act(() => hook.result.current.typing(''));
    act(() => hook.result.current.commit(false));
    expect(applied.at(-1)).toEqual({ kind: 'removeTextRun', page: 0, runId: 4 });
  });

  it('commits an open editor once, however many ways it is ended (a press beside it, then its blur)', () => {
    const { hook, applied } = setup();
    act(() => hook.result.current.open());
    act(() => hook.result.current.typing('Once'));
    act(() => hook.result.current.commitOpen());
    // The blur the press then causes, before the editor is closed.
    act(() => hook.result.current.commit(false));
    expect(applied).toEqual([{ kind: 'retypeTextRun', page: 0, runId: 4, text: 'Once' }]);
  });

  it('commits what is typed when asked from outside (a save)', () => {
    const { hook, applied } = setup();
    act(() => hook.result.current.open());
    act(() => hook.result.current.typing('Saved'));
    act(() => hook.result.current.commitOpen());
    expect(applied).toEqual([{ kind: 'retypeTextRun', page: 0, runId: 4, text: 'Saved' }]);
  });

  it('keeps the same typing callback across renders, so the editor need not re-run anything for it', () => {
    const { hook } = setup();
    const typing = hook.result.current.typing;
    act(() => hook.result.current.open());
    act(() => hook.result.current.typing('A'));
    expect(hook.result.current.typing).toBe(typing);
  });

  it('opens the editor in the look a queued restyle gives the run, not the one the file holds', async () => {
    const { hook } = setup({ look: { ...PLAIN, italic: true } });
    await waitFor(() => expect(hook.result.current.look?.italic).toBe(true));
    act(() => hook.result.current.open());
    expect(hook.result.current.editor?.look).toMatchObject({ bold: false, italic: true });
  });

  it('says why a run whose characters are unknown cannot be retyped', () => {
    const { hook, notices } = setup({ run: { ...hello, editable: false } });
    act(() => hook.result.current.open());
    expect(hook.result.current.editor).toBeNull();
    expect(notices).toEqual(['This text cannot be retyped: its font does not say what all of its characters are.']);
  });

  it('refuses the step when the page at the run’s position changed before its turn', async () => {
    const { hook } = setup();
    const moved: Session = { info: { ...info, pageIds: [10] }, annotations: [] };
    let built: (() => unknown) | null = null;
    // Re-run with a queue that builds against the moved session.
    const engine = fakeEngine({ textRunLook: async () => PLAIN });
    const content = {
      active: true,
      selected: { pageIndex: 0, key: '9/0/0/0', item: hello },
      following: false,
      follow: () => {},
    } as unknown as ContentMode;
    const other = renderHook(() =>
      useTextRunEditing({
        engine,
        handle: 1,
        info,
        content,
        queue: { pending: false, apply: async (build) => void (built = () => build(moved)), notify: () => {} },
        focusPages: () => {},
      }),
    );
    act(() => other.result.current.remove());
    expect(built).not.toBeNull();
    expect(() => built!()).toThrow('The page changed before the text could be edited. Nothing was changed.');
    hook.unmount();
  });
});

interface HarnessDeps {
  readonly engine: ReturnType<typeof fakeEngine>;
  readonly content: ContentMode;
  readonly queue: { pending: boolean; apply: (build: (session: Session) => Edit | null) => Promise<unknown>; notify: () => void };
}
let harnessDeps: HarnessDeps;

/** The viewer's wiring: the editor is mounted only while its page slot is (virtualized). */
function Harness({ slotMounted, onEditing }: { slotMounted: boolean; onEditing: (editing: ReturnType<typeof useTextRunEditing>) => void }) {
  const { engine, content, queue } = harnessDeps;
  const editing = useTextRunEditing({ engine, handle: 1, info, content, queue, focusPages: () => {} });
  onEditing(editing);
  return slotMounted && editing.editor ? (
    <TextRunEditor
      editor={editing.editor}
      scale={1}
      rotationDegrees={0}
      onCommit={editing.commit}
      onCancel={editing.cancel}
      onType={editing.typing}
      onAttach={editing.attach}
      onDetach={editing.detach}
    />
  ) : null;
}

describe('the open editor in a page slot that scrolls away', () => {
  function mountHarness() {
    const applied: (Edit | null)[] = [];
    harnessDeps = {
      engine: fakeEngine({ textRunLook: async () => PLAIN, textRunStyle: async () => null, textRunFontProgram: async () => null }),
      content: { active: true, selected: { pageIndex: 0, key: '9/0/0/0', item: hello }, following: false, follow: () => {} } as unknown as ContentMode,
      queue: { pending: false, apply: async (build) => void applied.push(build(session)), notify: () => {} },
    };
    let editing: ReturnType<typeof useTextRunEditing> | null = null;
    const onEditing = (value: ReturnType<typeof useTextRunEditing>) => void (editing = value);
    const view = render(<Harness slotMounted onEditing={onEditing} />);
    return {
      applied,
      current: () => editing!,
      remount: (slotMounted: boolean) => view.rerender(<Harness slotMounted={slotMounted} onEditing={onEditing} />),
    };
  }

  it('commits what was typed when its page slot unmounts, a turn later, and closes', async () => {
    const { applied, current, remount } = mountHarness();
    act(() => current().open());
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Scrolled' } });
    remount(false);
    // Not inside the unmount itself.
    expect(applied).toEqual([]);
    await waitFor(() => expect(applied).toEqual([{ kind: 'retypeTextRun', page: 0, runId: 4, text: 'Scrolled' }]));
    await waitFor(() => expect(current().editor).toBeNull());
    // Back in view: no editor showing the old text.
    remount(true);
    expect(screen.queryByRole('textbox')).toBeNull();
  });

  it('keeps the draft when the field is mounted again at once (a remount, not a scroll away)', async () => {
    const { applied, current, remount } = mountHarness();
    act(() => current().open());
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'Kept' } });
    act(() => {
      remount(false);
      remount(true);
    });
    await act(() => new Promise((resolve) => setTimeout(resolve, 10)));
    expect(applied).toEqual([]);
    expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('Kept');
  });

  it('removes the faces it loaded from the page when the document goes', async () => {
    const added = new Set<unknown>();
    Object.defineProperty(document, 'fonts', {
      value: { add: (face: unknown) => added.add(face), delete: (face: unknown) => added.delete(face) },
      configurable: true,
    });
    vi.stubGlobal(
      'FontFace',
      class {
        constructor(readonly family: string) {}
        load() {
          return Promise.resolve(this);
        }
      },
    );
    try {
      const engine = fakeEngine({
        textRunLook: async () => PLAIN,
        textRunStyle: async () => null,
        textRunFontProgram: async () => ({ format: 'trueType', bytes: new Uint8Array([1, 2]) }),
      });
      const content = { active: true, selected: { pageIndex: 0, key: '9/0/0/0', item: hello }, following: false, follow: () => {} } as unknown as ContentMode;
      const queue = { pending: false, apply: async () => {}, notify: () => {} };
      const hook = renderHook(() => useTextRunEditing({ engine, handle: 1, info, content, queue, focusPages: () => {} }));
      act(() => hook.result.current.open());
      await waitFor(() => expect(hook.result.current.editor?.family).not.toBeNull());
      expect(added.size).toBe(1);
      hook.unmount();
      expect(added.size).toBe(0);
    } finally {
      Reflect.deleteProperty(document, 'fonts');
    }
  });
});

describe('TextRunToolbar', () => {
  it('shows the look as pressed toggles, and offers off only what the core can take off', () => {
    const toggled: string[] = [];
    render(
      <TextRunToolbar
        run={hello}
        scale={1}
        look={{ ...PLAIN, bold: true, canRemoveBold: false, italic: true }}
        busy={false}
        onEdit={() => {}}
        onToggle={(face) => toggled.push(face)}
        onRemove={() => {}}
      />,
    );
    const bold = screen.getByRole('button', { name: 'Bold' });
    expect(bold.getAttribute('aria-pressed')).toBe('true');
    expect((bold as HTMLButtonElement).disabled).toBe(true);
    const italic = screen.getByRole('button', { name: 'Italic' });
    expect(italic.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(italic);
    fireEvent.click(screen.getByRole('button', { name: 'Underline' }));
    expect(toggled).toEqual(['italic', 'underline']);
  });

  it('offers no retype for a run it cannot retype, and nothing while a step runs', () => {
    const { rerender } = render(
      <TextRunToolbar run={{ ...hello, editable: false }} scale={1} look={PLAIN} busy={false} onEdit={() => {}} onToggle={() => {}} onRemove={() => {}} />,
    );
    expect((screen.getByRole('button', { name: 'Edit text' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Delete' }) as HTMLButtonElement).disabled).toBe(false);
    rerender(<TextRunToolbar run={hello} scale={1} look={PLAIN} busy onEdit={() => {}} onToggle={() => {}} onRemove={() => {}} />);
    expect((screen.getByRole('button', { name: 'Delete' }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('TextRunEditor', () => {
  const editor: OpenEditor = {
    run: hello,
    pageKey: '9/0',
    style: {
      baseFont: 'ABCDEF+Vera-Bold',
      fontSizePt: 14,
      color: { r: 200, g: 0, b: 0 },
      charSpacingPt: 0,
      horizontalScale: 1,
      bold: true,
      italic: false,
      renderMode: 0,
    },
    look: null,
    family: 'vitela-run-3',
    draft: 'Hello',
  };
  const noop = () => {};
  const quiet = { onCommit: noop, onCancel: noop, onType: noop, onAttach: noop, onDetach: noop };

  it("is drawn in the run's own font, size and colour over the run", () => {
    render(<TextRunEditor editor={editor} scale={2} rotationDegrees={0} {...quiet} />);
    const field = screen.getByRole('textbox', { name: 'Edit text “Hello”' }) as HTMLInputElement;
    expect(field.value).toBe('Hello');
    expect(document.activeElement).toBe(field);
    expect(field.style.fontFamily).toBe('vitela-run-3, Vera, sans-serif');
    expect(field.style.fontSize).toBe('28px');
    expect(field.style.fontWeight).toBe('700');
    expect(field.style.color).toBe('rgb(200, 0, 0)');
    expect(field.style.left).toBe('144px');
  });

  it('is drawn in the look a queued restyle gives the run', () => {
    render(<TextRunEditor editor={{ ...editor, look: { bold: false, italic: true, underline: false, strike: false } }} scale={1} rotationDegrees={0} {...quiet} />);
    const field = screen.getByRole('textbox') as HTMLInputElement;
    expect(field.style.fontWeight).toBe('400');
    expect(field.style.fontStyle).toBe('italic');
  });

  it('shows the draft it is given, reports each change, commits on Enter and on leaving, cancels on Escape', () => {
    const commits: boolean[] = [];
    let cancelled = 0;
    const typed: string[] = [];
    const props = {
      ...quiet,
      scale: 1,
      rotationDegrees: 0,
      onCommit: (refocus: boolean) => void commits.push(refocus),
      onCancel: () => void cancelled++,
      onType: (text: string) => void typed.push(text),
    };
    const { rerender } = render(<TextRunEditor editor={{ ...editor, draft: 'Typed before' }} {...props} />);
    const field = screen.getByRole('textbox') as HTMLInputElement;
    expect(field.value).toBe('Typed before');
    fireEvent.change(field, { target: { value: 'Howdy' } });
    expect(typed).toEqual(['Howdy']);
    rerender(<TextRunEditor editor={{ ...editor, draft: 'Howdy' }} {...props} />);
    expect(field.value).toBe('Howdy');
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(commits).toEqual([true]);
    // Which of these counts is the hook's to say (it commits once): the field reports both.
    fireEvent.blur(field);
    expect(commits).toEqual([true, false]);
    fireEvent.keyDown(field, { key: 'Escape' });
    expect(cancelled).toBe(1);
  });

  it('tells the hook when it is mounted and when it goes', () => {
    const calls: string[] = [];
    const { unmount } = render(
      <TextRunEditor editor={editor} scale={1} rotationDegrees={0} {...quiet} onAttach={() => calls.push('attach')} onDetach={() => calls.push('detach')} />,
    );
    expect(calls).toEqual(['attach']);
    unmount();
    expect(calls).toEqual(['attach', 'detach']);
  });
});
