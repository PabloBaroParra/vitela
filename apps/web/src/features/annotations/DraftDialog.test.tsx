import { act, cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { EngineError, ENCODING_GAP } from '@vitela/core';
import type { FreeTextLayout } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { DraftDialog } from './DraftDialog';
import type { Draft } from './useDrawing';

// jsdom has no modal <dialog>: model just enough of it for the component.
beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
afterEach(cleanup);

const draft = (overrides: Partial<Draft> = {}): Draft => ({
  tool: 'note',
  pageIndex: 0,
  pageKey: '0/0',
  rect: { left: 10, top: 10, width: 200, height: 50 },
  frame: { width: 200, height: 50 },
  busy: false,
  error: null,
  ...overrides,
});

const LAYOUT: FreeTextLayout = {
  fontSizePt: 12,
  lines: [{ text: 'Hello there', xPt: 2, baselineFromTopPt: 12.6 }],
  overflow: false,
};

function dialog(shown: Draft, freeTextLayout = vi.fn(async () => LAYOUT)) {
  const onSubmit = vi.fn();
  const onCancel = vi.fn();
  const onChange = vi.fn();
  const view = render(
    <DraftDialog
      engine={fakeEngine({ freeTextLayout })}
      draft={shown}
      onChange={onChange}
      onSubmit={onSubmit}
      onCancel={onCancel}
    />,
  );
  const field = view.getByRole('textbox') as HTMLTextAreaElement;
  return { ...view, field, onSubmit, onCancel, onChange, freeTextLayout };
}

describe('DraftDialog', () => {
  it('asks for a note’s text with its field focused, and adds it', () => {
    const { getByRole, field, onSubmit, onChange } = dialog(draft());
    expect(getByRole('dialog', { name: 'Add a note' })).toBeTruthy();
    expect(document.activeElement).toBe(field);
    const add = getByRole('button', { name: 'Add note' });
    expect(add).toHaveProperty('disabled', true);

    fireEvent.change(field, { target: { value: 'Call back' } });
    expect(onChange).toHaveBeenCalled();
    fireEvent.click(add);
    expect(onSubmit).toHaveBeenCalledWith('Call back');
  });

  it('adds with Ctrl+Enter, and Escape cancels', () => {
    const { getByRole, field, onSubmit, onCancel } = dialog(draft());
    fireEvent.change(field, { target: { value: 'two\nlines' } });
    // jsdom has no requestSubmit: submit the form the way it would.
    const form = field.form!;
    form.requestSubmit ??= function (this: HTMLFormElement) {
      fireEvent.submit(this);
    };
    fireEvent.keyDown(field, { key: 'Enter', ctrlKey: true });
    expect(onSubmit).toHaveBeenCalledWith('two\nlines');
    fireEvent(getByRole('dialog'), new Event('cancel', { cancelable: true }));
    expect(onCancel).toHaveBeenCalled();
  });

  it('shows the core’s layout of a text box as it is typed, in the box’s frame', async () => {
    const { getByRole, field, freeTextLayout, container } = dialog(draft({ tool: 'freeText', frame: { width: 50, height: 200 } }));
    expect(getByRole('dialog', { name: 'Add a text box' })).toBeTruthy();
    fireEvent.change(field, { target: { value: 'Hello there' } });
    await waitFor(() => expect(container.querySelector('.free-text-preview text')?.textContent).toBe('Hello there'));
    expect(freeTextLayout).toHaveBeenLastCalledWith('Hello there', 50, 200);
    expect(getByRole('img', { name: 'Text box preview' }).getAttribute('viewBox')).toBe('0 0 50 200');
  });

  it('shows at once a character the core will not lay out', async () => {
    const gap = vi.fn(() => Promise.reject(new EngineError(ENCODING_GAP, "'日' cannot be shown in Helvetica (WinAnsi)")));
    const { field, findByRole } = dialog(draft({ tool: 'freeText' }), gap);
    fireEvent.change(field, { target: { value: '日' } });
    expect((await findByRole('alert')).textContent).toBe("'日' cannot be shown in Helvetica (WinAnsi)");
    expect(field.getAttribute('aria-invalid')).toBe('true');
  });

  it('keeps the core’s refusal on screen and waits while adding', () => {
    const { getByRole } = dialog(draft({ tool: 'freeText', busy: true, error: 'no glyph' }));
    expect(getByRole('alert').textContent).toBe('no glyph');
    expect(getByRole('button', { name: 'Add text box' })).toHaveProperty('disabled', true);
  });

  it('cannot be cancelled while its text is being added: Cancel waits, Escape is ignored', () => {
    const { getByRole, onCancel } = dialog(draft({ busy: true }));
    const cancel = getByRole('button', { name: 'Cancel' });
    expect(cancel).toHaveProperty('disabled', true);
    fireEvent.click(cancel);
    const escape = new Event('cancel', { cancelable: true });
    fireEvent(getByRole('dialog'), escape);
    expect(escape.defaultPrevented).toBe(true);
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('drops a layout that arrives for text since changed', async () => {
    vi.useFakeTimers();
    try {
      let answer: (layout: FreeTextLayout) => void = () => {};
      const slow = vi.fn(() => new Promise<FreeTextLayout>((resolve) => (answer = resolve)));
      const { field, container } = dialog(draft({ tool: 'freeText' }), slow);
      fireEvent.change(field, { target: { value: 'first' } });
      await act(() => vi.advanceTimersByTimeAsync(200));
      fireEvent.change(field, { target: { value: 'second' } });
      await act(async () => answer(LAYOUT));
      expect(container.querySelector('.free-text-preview text')).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});
