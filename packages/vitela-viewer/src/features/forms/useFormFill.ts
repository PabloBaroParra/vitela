// Filling a form field in: one undo step through the viewer's edit queue, so
// undo, redo and save see it like any other edit. Container logic only.
//
// The core records a fill with the value the field already holds as a step
// all the same, so it is never sent: compared once when asked (with the last
// value asked for that field while one is still pending, else the shown one:
// a quick revert must not be lost behind a fill that has not landed), and
// again inside its step against the fields as the core last listed them (a
// fill still queued may have set it meanwhile).
//
// Whether the document lets its fields be filled is known at open
// (`formFillAllowed`) and is asked again on every fill (`EditNotPermitted`,
// after which the panel is read-only). A value the field cannot hold is the
// core's to refuse (`UnsupportedOperation`): its words stay with that field
// until it is filled again.

import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { EDIT_NOT_PERMITTED, FORM_FIELD_NOT_FOUND } from '@vitela/core';
import type { FieldValue, FormField } from '@vitela/core';
import type { useAnnotationEdits } from '../annotations/useAnnotationEdits';
import { sameFieldValue, sameValue } from './fieldValues';

/** The core's word for a value the field cannot hold (it has no constant in `@vitela/core`). */
const UNSUPPORTED_OPERATION = 'UnsupportedOperation';

/** What the panel says itself, next to the field (or by going read-only). */
const SHOWN_BY_PANEL = [EDIT_NOT_PERMITTED, FORM_FIELD_NOT_FOUND, UNSUPPORTED_OPERATION];

/** The part of the viewer's edit queue a fill goes through. */
export type FormFillQueue = Pick<ReturnType<typeof useAnnotationEdits>, 'apply'>;

/** `shown`: the fields on screen, what a step checks against until one has listed them. */
export function useFormFill(editing: FormFillQueue, allowedAtOpen: boolean, shown: readonly FormField[]) {
  const [denied, setDenied] = useState(false);
  const [errors, setErrors] = useState<ReadonlyMap<number, string>>(new Map());
  const shownNow = useRef(shown);
  useLayoutEffect(() => {
    shownNow.current = shown;
  });
  /** The last value asked for each field whose fill has not ended yet. */
  const asked = useRef(new Map<number, { readonly value: FieldValue }>());
  const allowed = allowedAtOpen && !denied;
  const { apply } = editing;

  const setError = useCallback((id: number, message: string | null) => {
    setErrors((current) => {
      if (!message && !current.has(id)) return current;
      const next = new Map(current);
      if (message) next.set(id, message);
      else next.delete(id);
      return next;
    });
  }, []);

  /**
   * Fills `field` (as it was shown) with `value`, once the steps before it
   * have ended. `true` when the field holds it now (also when it already
   * did), `false` when it was not filled.
   */
  const fill = useCallback(
    async (field: FormField, value: FieldValue): Promise<boolean> => {
      if (!allowed) return false;
      setError(field.id, null);
      const pending = asked.current.get(field.id);
      if (pending ? sameFieldValue(pending.value, value) : sameValue(field, value)) return true;
      const token = { value };
      asked.current.set(field.id, token);
      const outcome = await apply((session) => {
        const now = (session.formFields ?? shownNow.current).find(({ id }) => id === field.id);
        if (now && sameValue(now, value)) return null;
        return { kind: 'setFieldValue', fieldId: field.id, value };
      }, SHOWN_BY_PANEL).finally(() => {
        if (asked.current.get(field.id) === token) asked.current.delete(field.id);
      });
      if (outcome.status !== 'failed') return outcome.status === 'changed';
      if (outcome.kind === EDIT_NOT_PERMITTED) setDenied(true);
      else if (outcome.kind && SHOWN_BY_PANEL.includes(outcome.kind)) setError(field.id, outcome.message);
      return false;
    },
    [allowed, apply, setError],
  );

  return { allowed, errors, fill };
}
