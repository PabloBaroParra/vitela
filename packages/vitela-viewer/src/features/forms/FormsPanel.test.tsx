import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { FieldValue, FormField } from '@vitela/core';
import { ALL_FIELDS, COUNTRY, NAME, NOTES, PLAN, SUBSCRIBE } from '../../test/formFields';
import { FormsPanel } from './FormsPanel';
import type { FormFieldsState } from './useFormFields';

afterEach(cleanup);

interface Options {
  readonly fields?: readonly FormField[];
  readonly state?: FormFieldsState;
  readonly allowed?: boolean;
  readonly errors?: ReadonlyMap<number, string>;
  readonly fill?: (field: FormField, value: FieldValue) => Promise<boolean>;
}

function panel({ fields = ALL_FIELDS, state, allowed = true, errors = new Map(), fill }: Options = {}) {
  const onFill = vi.fn(fill ?? (async () => true));
  const onFocusField = vi.fn();
  const onBlurField = vi.fn();
  const onRetry = vi.fn();
  const onClose = vi.fn();
  const element = (shownErrors: ReadonlyMap<number, string>) => (
    <FormsPanel
      id="forms"
      state={state ?? { status: 'ready', fields }}
      allowed={allowed}
      errors={shownErrors}
      onFill={onFill}
      onFocusField={onFocusField}
      onBlurField={onBlurField}
      onRetry={onRetry}
      onClose={onClose}
    />
  );
  const view = render(element(errors));
  const showErrors = (shownErrors: ReadonlyMap<number, string>) => view.rerender(element(shownErrors));
  return { ...view, showErrors, onFill, onFocusField, onBlurField, onRetry, onClose };
}

describe('FormsPanel', () => {
  it('lists every field by page, each control labelled with its name, in the order of the list', () => {
    const { getByRole, getByLabelText, container } = panel();
    expect(getByRole('complementary', { name: 'Form fields' })).toBeTruthy();
    expect([...container.querySelectorAll('h3')].map((heading) => heading.textContent)).toEqual(['Page 1', 'Page 2']);
    expect((getByLabelText('full_name') as HTMLInputElement).value).toBe('Ada Lovelace');
    expect(getByLabelText('full_name').getAttribute('maxlength')).toBe('100');
    expect(getByLabelText('notes').tagName).toBe('TEXTAREA');
    expect((getByLabelText('subscribe') as HTMLInputElement).checked).toBe(true);
    expect(getByRole('radiogroup', { name: 'plan' })).toBeTruthy();
    expect((getByRole('radio', { name: 'pro' }) as HTMLInputElement).checked).toBe(true);
    expect((getByLabelText('country') as HTMLSelectElement).value).toBe('Uruguay');
    // Tab goes down the list: no control takes itself out of the order.
    const controls = [...container.querySelectorAll('input, textarea, select')];
    expect(controls.map((control) => control.getAttribute('tabindex'))).toEqual(controls.map(() => null));
  });

  it('fills a text field in when the focus leaves it, and only if it changed', async () => {
    const { getByLabelText, onFill } = panel();
    const name = getByLabelText('full_name');
    fireEvent.focus(name);
    await act(async () => {
      fireEvent.blur(name);
    });
    expect(onFill).not.toHaveBeenCalled();

    fireEvent.focus(name);
    fireEvent.change(name, { target: { value: 'Grace Hopper' } });
    expect(onFill).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.blur(name);
    });
    expect(onFill).toHaveBeenCalledWith(NAME, { kind: 'text', text: 'Grace Hopper' });
  });

  it('fills a one-line field on Enter, keeping the focus there; a multiline one takes Enter as a new line', async () => {
    const { getByLabelText, onFill } = panel();
    const name = getByLabelText('full_name');
    name.focus();
    fireEvent.change(name, { target: { value: 'Grace' } });
    await act(async () => {
      fireEvent.keyDown(name, { key: 'Enter' });
    });
    expect(onFill).toHaveBeenCalledWith(NAME, { kind: 'text', text: 'Grace' });
    expect(document.activeElement).toBe(name);

    const notes = getByLabelText('notes');
    fireEvent.change(notes, { target: { value: 'one' } });
    await act(async () => {
      fireEvent.keyDown(notes, { key: 'Enter' });
    });
    expect(onFill).toHaveBeenCalledTimes(1);
    await act(async () => {
      fireEvent.blur(notes);
    });
    expect(onFill).toHaveBeenLastCalledWith(NOTES, { kind: 'text', text: 'one' });
  });

  it('puts the text back on Escape without closing; Escape on an unchanged field closes', async () => {
    const { getByLabelText, onFill, onClose } = panel();
    const name = getByLabelText('full_name') as HTMLInputElement;
    fireEvent.change(name, { target: { value: 'Grace' } });
    fireEvent.keyDown(name, { key: 'Escape' });
    expect(name.value).toBe('Ada Lovelace');
    expect(onClose).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.blur(name);
    });
    expect(onFill).not.toHaveBeenCalled();
    fireEvent.keyDown(name, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('keeps refused text in the field, with the core reason tied to it', async () => {
    const { getByLabelText, getByRole, showErrors } = panel({ fill: async () => false });
    const name = getByLabelText('full_name') as HTMLInputElement;
    fireEvent.change(name, { target: { value: 'too long' } });
    await act(async () => {
      fireEvent.blur(name);
    });
    expect(name.value).toBe('too long');
    showErrors(new Map([[1, 'text longer than /MaxLen']]));
    const alert = getByRole('alert');
    expect(alert.textContent).toBe('text longer than /MaxLen');
    expect(name.getAttribute('aria-invalid')).toBe('true');
    expect(name.getAttribute('aria-describedby')).toBe(alert.id);
  });

  it('fills a checkbox, a radio group and a dropdown as soon as they change', async () => {
    const { getByLabelText, getByRole, onFill } = panel();
    await act(async () => {
      fireEvent.click(getByLabelText('subscribe'));
    });
    expect(onFill).toHaveBeenLastCalledWith(SUBSCRIBE, { kind: 'checked', checked: false });
    await act(async () => {
      fireEvent.click(getByRole('radio', { name: 'basic' }));
    });
    expect(onFill).toHaveBeenLastCalledWith(PLAN, { kind: 'choice', option: 'basic' });
    await act(async () => {
      fireEvent.change(getByLabelText('country'), { target: { value: 'Chile' } });
    });
    expect(onFill).toHaveBeenLastCalledWith(COUNTRY, { kind: 'choice', option: 'Chile' });
    await act(async () => {
      fireEvent.change(getByLabelText('country'), { target: { value: '' } });
    });
    expect(onFill).toHaveBeenLastCalledWith(COUNTRY, { kind: 'choice', option: null });
  });

  it('shows a choice made at once, and what the document holds once the fill has ended', async () => {
    let finish!: (filled: boolean) => void;
    const { getByLabelText } = panel({ fill: () => new Promise((resolve) => (finish = resolve)) });
    const box = getByLabelText('subscribe') as HTMLInputElement;
    await act(async () => {
      fireEvent.click(box);
    });
    expect(box.checked).toBe(false);
    // Refused: the field still holds what it held.
    await act(async () => finish(false));
    expect(box.checked).toBe(true);
  });

  it('takes an editable dropdown as typed text, filled in when the focus leaves it', async () => {
    const editable = { ...COUNTRY, editable: true };
    const { getByLabelText, container, onFill } = panel({ fields: [editable] });
    const input = getByLabelText('country') as HTMLInputElement;
    expect(input.tagName).toBe('INPUT');
    const list = container.querySelector(`datalist[id="${input.getAttribute('list')}"]`)!;
    expect([...list.querySelectorAll('option')].map((option) => option.value)).toEqual(['Argentina', 'Uruguay', 'Chile']);
    fireEvent.change(input, { target: { value: 'Peru' } });
    await act(async () => {
      fireEvent.blur(input);
    });
    expect(onFill).toHaveBeenCalledWith(editable, { kind: 'choice', option: 'Peru' });
  });

  it('tells which field has the focus, and where: a radio button by its own box', () => {
    const { getByLabelText, getByRole, onFocusField, onBlurField } = panel();
    fireEvent.focus(getByLabelText('full_name'));
    expect(onFocusField).toHaveBeenLastCalledWith(NAME, NAME.rect);
    fireEvent.blur(getByLabelText('full_name'));
    expect(onBlurField).toHaveBeenCalledTimes(1);
    fireEvent.focus(getByRole('radio', { name: 'basic' }));
    expect(onFocusField).toHaveBeenLastCalledWith(PLAN, { left: 72, top: 280, width: 14, height: 14 });
  });

  it('is read-only where the document does not allow filling in, and says so', async () => {
    const { getByLabelText, getByRole, getByText, onFill } = panel({ allowed: false });
    expect(getByText('This document does not allow its form fields to be filled in.')).toBeTruthy();
    expect(getByLabelText('full_name').hasAttribute('readonly')).toBe(true);
    expect(getByLabelText('notes').hasAttribute('readonly')).toBe(true);
    expect((getByLabelText('subscribe') as HTMLInputElement).disabled).toBe(true);
    expect((getByRole('radio', { name: 'basic' }) as HTMLInputElement).disabled).toBe(true);
    expect((getByLabelText('country') as HTMLSelectElement).disabled).toBe(true);
    fireEvent.change(getByLabelText('full_name'), { target: { value: 'Grace' } });
    await act(async () => {
      fireEvent.blur(getByLabelText('full_name'));
    });
    expect(onFill).not.toHaveBeenCalled();
  });

  it('sends a text put back to what the field held while its first fill is still pending', async () => {
    const { getByLabelText, onFill } = panel({ fill: () => new Promise<boolean>(() => {}) });
    const name = getByLabelText('full_name');
    fireEvent.change(name, { target: { value: 'Grace' } });
    await act(async () => {
      fireEvent.blur(name);
    });
    fireEvent.change(name, { target: { value: 'Ada Lovelace' } });
    await act(async () => {
      fireEvent.blur(name);
    });
    expect(onFill.mock.calls.map(([, value]) => value)).toEqual([
      { kind: 'text', text: 'Grace' },
      { kind: 'text', text: 'Ada Lovelace' },
    ]);
  });

  it('says when there are none, while they load, and why they could not be listed', () => {
    const empty = panel({ fields: [] });
    expect(empty.getByText('This document has no form fields.')).toBeTruthy();
    cleanup();
    const loading = panel({ state: { status: 'loading', fields: [] } });
    expect(loading.getByRole('status').textContent).toBe('Loading form fields…');
    cleanup();
    const failed = panel({ state: { status: 'failed', fields: [], message: 'worker gone' } });
    expect(failed.getByRole('alert').textContent).toBe('Could not list the form fields: worker gone');
    fireEvent.click(failed.getByRole('button', { name: 'Try again' }));
    expect(failed.onRetry).toHaveBeenCalledTimes(1);
  });

  it('keeps the focus in the panel when Try again takes the failure away', () => {
    const element = (state: FormFieldsState) => (
      <FormsPanel
        id="forms"
        state={state}
        allowed
        errors={new Map()}
        onFill={vi.fn()}
        onFocusField={vi.fn()}
        onBlurField={vi.fn()}
        onRetry={vi.fn()}
        onClose={vi.fn()}
      />
    );
    const view = render(element({ status: 'failed', fields: [], message: 'worker gone' }));
    view.getByRole('button', { name: 'Try again' }).focus();
    view.rerender(element({ status: 'loading', fields: [] }));
    expect(document.activeElement).toBe(view.getByRole('complementary', { name: 'Form fields' }));
  });

  it('closes on Escape and from its button', () => {
    const { getByRole, onClose } = panel();
    fireEvent.keyDown(getByRole('complementary', { name: 'Form fields' }), { key: 'Escape' });
    fireEvent.click(getByRole('button', { name: 'Close form fields' }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
