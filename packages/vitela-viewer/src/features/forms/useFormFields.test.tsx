import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { FormField } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { ALL_FIELDS, NAME } from '../../test/formFields';
import { useFormFields } from './useFormFields';

describe('useFormFields', () => {
  it('asks the engine for the fields once, while no edit has listed them', async () => {
    const formFields = vi.fn(async () => [...ALL_FIELDS]);
    const engine = fakeEngine({ formFields });
    const { result, rerender } = renderHook(() => useFormFields(engine, 4, undefined));
    expect(result.current.state).toEqual({ status: 'loading', fields: [] });
    await waitFor(() => expect(result.current.state).toEqual({ status: 'ready', fields: ALL_FIELDS }));
    rerender();
    expect(formFields).toHaveBeenCalledTimes(1);
    expect(formFields).toHaveBeenCalledWith(4);
  });

  it('shows what the last edit listed, without asking', () => {
    const formFields = vi.fn(async () => [] as FormField[]);
    const listed = [{ ...NAME, value: 'Grace Hopper' }];
    const { result } = renderHook(() => useFormFields(fakeEngine({ formFields }), 4, listed));
    expect(result.current.state).toEqual({ status: 'ready', fields: listed });
    expect(formFields).not.toHaveBeenCalled();
  });

  it('says why the fields could not be listed, and asks again on retry', async () => {
    let calls = 0;
    const engine = fakeEngine({
      formFields: async () => {
        if (++calls === 1) throw new Error('worker gone');
        return [NAME];
      },
    });
    const { result } = renderHook(() => useFormFields(engine, 4, undefined));
    await waitFor(() => expect(result.current.state).toEqual({ status: 'failed', fields: [], message: 'worker gone' }));
    act(() => result.current.retry());
    await waitFor(() => expect(result.current.state).toEqual({ status: 'ready', fields: [NAME] }));
  });

  it('drops an answer for a document no longer shown', async () => {
    let answer!: (fields: FormField[]) => void;
    const engine = fakeEngine({
      formFields: (handle) => (handle === 1 ? new Promise((resolve) => (answer = resolve)) : Promise.resolve([NAME])),
    });
    const { result, rerender } = renderHook(({ handle }) => useFormFields(engine, handle, undefined), { initialProps: { handle: 1 } });
    rerender({ handle: 2 });
    await waitFor(() => expect(result.current.state).toEqual({ status: 'ready', fields: [NAME] }));
    await act(async () => answer([...ALL_FIELDS]));
    expect(result.current.state).toEqual({ status: 'ready', fields: [NAME] });
  });
});
