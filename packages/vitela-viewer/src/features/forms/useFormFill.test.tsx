import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { EDIT_NOT_PERMITTED, EngineError } from '@vitela/core';
import type { Edit, FieldValue, FormField } from '@vitela/core';
import type { ChangeOutcome, Session } from '../annotations/useAnnotationEdits';
import { NAME, PLAN, SUBSCRIBE } from '../../test/formFields';
import { useFormFill, type FormFillQueue } from './useFormFill';

const session = (formFields?: readonly FormField[]): Session => ({ info: { pages: [], pageIds: [] }, annotations: [], formFields });

/** A queue that runs each step at once over `now`, and records what it would send. */
function queue(now: Session, fail?: Error) {
  const sent: Edit[] = [];
  const shown: (readonly string[])[] = [];
  const apply = vi.fn(async (build: (session: Session) => Edit | null, shownByCaller: readonly string[] = []): Promise<ChangeOutcome> => {
    shown.push(shownByCaller);
    const edit = build(now);
    if (!edit) return { status: 'changed' };
    if (fail) return { status: 'failed', kind: fail.name, message: fail.message };
    sent.push(edit);
    return { status: 'changed' };
  });
  return { editing: { apply } satisfies FormFillQueue, sent, shown, apply };
}

/**
 * A queue that runs its steps one after another, each held until `release()`,
 * over a document whose fields each sent fill changes.
 */
function heldQueue(fields: readonly FormField[]) {
  let document = fields;
  const sent: Edit[] = [];
  const held: (() => void)[] = [];
  const runs: Promise<unknown>[] = [];
  let tail: Promise<unknown> = Promise.resolve();
  const apply = vi.fn((build: (session: Session) => Edit | null): Promise<ChangeOutcome> => {
    const run = tail.then(async (): Promise<ChangeOutcome> => {
      await new Promise<void>((resolve) => held.push(resolve));
      const edit = build(session(document));
      if (!edit || edit.kind !== 'setFieldValue') return { status: 'changed' };
      sent.push(edit);
      document = document.map((field) => (field.id === edit.fieldId ? withValue(field, edit.value) : field));
      return { status: 'changed' };
    });
    tail = run;
    runs.push(run);
    return run;
  });
  /** Lets the next held step run. */
  let released = 0;
  const release = async () => {
    for (let tick = 0; held.length === 0 && tick < 50; tick++) await Promise.resolve();
    held.shift()?.();
    await runs[released++];
  };
  return { editing: { apply } satisfies FormFillQueue, sent, release, document: () => document };
}

function withValue(field: FormField, value: FieldValue): FormField {
  if (field.kind === 'text' && value.kind === 'text') return { ...field, value: value.text };
  if (field.kind === 'checkbox' && value.kind === 'checked') return { ...field, checked: value.checked };
  if ((field.kind === 'radio' || field.kind === 'dropdown') && value.kind === 'choice') return { ...field, selected: value.option };
  return field;
}

describe('useFormFill', () => {
  it('fills a field with a value it does not hold', async () => {
    const { editing, sent } = queue(session([NAME]));
    const { result } = renderHook(() => useFormFill(editing, true, [NAME]));
    let filled = false;
    await act(async () => {
      filled = await result.current.fill(NAME, { kind: 'text', text: 'Grace Hopper' });
    });
    expect(filled).toBe(true);
    expect(sent).toEqual([{ kind: 'setFieldValue', fieldId: 1, value: { kind: 'text', text: 'Grace Hopper' } }]);
  });

  it('sends nothing for the value the field already holds: the core would record a step', async () => {
    const { editing, apply } = queue(session([SUBSCRIBE]));
    const { result } = renderHook(() => useFormFill(editing, true, [SUBSCRIBE]));
    await act(() => result.current.fill(SUBSCRIBE, { kind: 'checked', checked: true }));
    expect(apply).not.toHaveBeenCalled();
  });

  it('checks again inside its step, against the fields as the core last listed them', async () => {
    // An earlier step (still queued when this one was asked for) chose "basic" already.
    const { editing, sent } = queue(session([{ ...PLAN, selected: 'basic' }]));
    const { result } = renderHook(() => useFormFill(editing, true, [PLAN]));
    await act(() => result.current.fill(PLAN, { kind: 'choice', option: 'basic' }));
    expect(sent).toEqual([]);
  });

  it('checks against the fields it was handed while no step has listed them', async () => {
    const { editing, sent } = queue(session(undefined));
    const { result } = renderHook(() => useFormFill(editing, true, [{ ...PLAN, selected: 'basic' }]));
    await act(() => result.current.fill(PLAN, { kind: 'choice', option: 'basic' }));
    expect(sent).toEqual([]);
  });

  it('keeps the core refusal of a value with its field, until that field is filled again', async () => {
    const { editing, shown } = queue(session([NAME]), new EngineError('UnsupportedOperation', 'text longer than /MaxLen'));
    const { result } = renderHook(() => useFormFill(editing, true, [NAME]));
    let filled = true;
    await act(async () => {
      filled = await result.current.fill(NAME, { kind: 'text', text: 'x'.repeat(101) });
    });
    expect(filled).toBe(false);
    expect(result.current.errors.get(1)).toBe('text longer than /MaxLen');
    expect(shown[0]).toContain('UnsupportedOperation');
    await act(() => result.current.fill(NAME, { kind: 'text', text: 'Ada Lovelace' }));
    expect(result.current.errors.get(1)).toBeUndefined();
  });

  it('turns read-only when the core refuses to fill', async () => {
    const { editing } = queue(session([NAME]), new EngineError(EDIT_NOT_PERMITTED, 'not permitted'));
    const { result } = renderHook(() => useFormFill(editing, true, [NAME]));
    expect(result.current.allowed).toBe(true);
    await act(() => result.current.fill(NAME, { kind: 'text', text: 'Grace' }));
    expect(result.current.allowed).toBe(false);
    expect(result.current.errors.size).toBe(0);
  });

  it('sends a quick revert as its own fill: checked then unchecked before the first lands', async () => {
    const unchecked = { ...SUBSCRIBE, checked: false };
    const { editing, sent, release, document } = heldQueue([unchecked]);
    const { result } = renderHook(() => useFormFill(editing, true, [unchecked]));
    let first!: Promise<boolean>;
    let second!: Promise<boolean>;
    act(() => {
      // Both asked for while the field is still shown unchecked.
      first = result.current.fill(unchecked, { kind: 'checked', checked: true });
      second = result.current.fill(unchecked, { kind: 'checked', checked: false });
    });
    await act(async () => {
      await release();
      await release();
      await Promise.all([first, second]);
    });
    expect(sent.map((edit) => edit.kind === 'setFieldValue' && edit.value)).toEqual([
      { kind: 'checked', checked: true },
      { kind: 'checked', checked: false },
    ]);
    expect(document()).toEqual([unchecked]);
  });

  it('sends a text put back to what the field held while the first fill is pending', async () => {
    const { editing, sent, release, document } = heldQueue([NAME]);
    const { result } = renderHook(() => useFormFill(editing, true, [NAME]));
    let first!: Promise<boolean>;
    let second!: Promise<boolean>;
    act(() => {
      first = result.current.fill(NAME, { kind: 'text', text: 'Grace' });
      second = result.current.fill(NAME, { kind: 'text', text: 'Ada Lovelace' });
    });
    await act(async () => {
      await release();
      await release();
      await Promise.all([first, second]);
    });
    expect(sent).toHaveLength(2);
    expect(document()).toEqual([NAME]);
  });

  it('fills nothing where the document does not allow it', async () => {
    const { editing, apply } = queue(session([NAME]));
    const { result } = renderHook(() => useFormFill(editing, false, [NAME]));
    expect(result.current.allowed).toBe(false);
    await act(() => result.current.fill(NAME, { kind: 'text', text: 'Grace' }));
    expect(apply).not.toHaveBeenCalled();
  });
});
