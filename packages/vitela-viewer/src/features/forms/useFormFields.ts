// The document's form fields: what the last edit, undo or redo listed, or,
// until one has, what the engine answers when asked (once per document; again
// on retry). Container logic only.

import { useCallback, useEffect, useState } from 'react';
import { errorMessage } from '@vitela/core';
import type { DocumentHandle, Engine, FormField } from '@vitela/core';

export type FormFieldsState =
  | { readonly status: 'loading' | 'ready'; readonly fields: readonly FormField[] }
  | { readonly status: 'failed'; readonly fields: readonly FormField[]; readonly message: string };

const LOADING: FormFieldsState = { status: 'loading', fields: [] };

interface Answer {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  readonly attempt: number;
  readonly state: FormFieldsState;
}

/** `listed`: the fields the last edited document brought; unset until one did. */
export function useFormFields(engine: Engine, handle: DocumentHandle, listed: readonly FormField[] | undefined) {
  const [answer, setAnswer] = useState<Answer | null>(null);
  const [attempt, setAttempt] = useState(0);
  const asking = listed === undefined;

  useEffect(() => {
    if (!asking) return;
    let current = true;
    engine.formFields(handle).then(
      (fields) => current && setAnswer({ engine, handle, attempt, state: { status: 'ready', fields } }),
      (error: unknown) =>
        current && setAnswer({ engine, handle, attempt, state: { status: 'failed', fields: [], message: errorMessage(error) } }),
    );
    return () => {
      current = false;
    };
  }, [asking, engine, handle, attempt]);

  const retry = useCallback(() => setAttempt((count) => count + 1), []);
  if (listed) return { state: { status: 'ready', fields: listed } as FormFieldsState, retry };
  const fits = answer && answer.engine === engine && answer.handle === handle && answer.attempt === attempt;
  return { state: fits ? answer.state : LOADING, retry };
}
