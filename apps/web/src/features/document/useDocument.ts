// Opening a document: reads the picked file, hands its bytes to the engine,
// asks for a password when the core says one is needed, and tracks what the
// UI should show. Container logic only, no markup.

import { useCallback, useRef, useState } from 'react';
import { PASSWORD_REQUIRED, WRONG_PASSWORD, errorKind, errorMessage } from '../../core/errors';
import type { DocumentInfo, Engine } from '../../core/types';

export interface OpenDocument {
  readonly name: string;
  readonly info: DocumentInfo;
  /** Bumps on every successful open, so views reset for a same-named file. */
  readonly generation: number;
}

/** What is happening besides showing the open document, if any. */
export type Activity =
  | { readonly kind: 'idle' }
  | { readonly kind: 'opening'; readonly name: string }
  | {
      readonly kind: 'password';
      readonly name: string;
      /** The last password tried did not open the document. */
      readonly wrong: boolean;
      /** A password is being checked right now. */
      readonly checking: boolean;
    }
  | { readonly kind: 'failed'; readonly name: string; readonly message: string };

export interface DocumentState {
  /**
   * The document the engine has open. A failed or cancelled open leaves it
   * in place: the engine only closes it once a new one has opened.
   */
  readonly document: OpenDocument | null;
  readonly activity: Activity;
}

const IDLE: Activity = { kind: 'idle' };

export function useDocument(engine: Engine) {
  const [state, setState] = useState<DocumentState>({ document: null, activity: IDLE });
  const latest = useRef(0);
  /** The file waiting for a password; `null` when no prompt is open. */
  const locked = useRef<{ file: File; attempt: number } | null>(null);

  const attempt = useCallback(
    (file: File, attemptId: number, password: string | undefined) => {
      const current = () => attemptId === latest.current;
      // Read afresh each time: the engine takes the bytes by transfer.
      file
        .arrayBuffer()
        .then((buffer) => engine.open(new Uint8Array(buffer), password))
        .then(
          (info) => {
            if (!current()) return;
            locked.current = null;
            setState({ document: { name: file.name, info, generation: attemptId }, activity: IDLE });
          },
          (error: unknown) => {
            if (!current()) return;
            const kind = errorKind(error);
            if (kind === PASSWORD_REQUIRED || kind === WRONG_PASSWORD) {
              locked.current = { file, attempt: attemptId };
              const wrong = password !== undefined;
              setState((s) => ({ ...s, activity: { kind: 'password', name: file.name, wrong, checking: false } }));
            } else {
              locked.current = null;
              setState((s) => ({ ...s, activity: { kind: 'failed', name: file.name, message: errorMessage(error) } }));
            }
          },
        );
    },
    [engine],
  );

  const open = useCallback(
    (file: File) => {
      const attemptId = ++latest.current;
      locked.current = null;
      setState((s) => ({ ...s, activity: { kind: 'opening', name: file.name } }));
      attempt(file, attemptId, undefined);
    },
    [attempt],
  );

  const submitPassword = useCallback(
    (password: string) => {
      const pending = locked.current;
      if (!pending || pending.attempt !== latest.current) return;
      setState((s) =>
        s.activity.kind === 'password' ? { ...s, activity: { ...s.activity, checking: true } } : s,
      );
      attempt(pending.file, pending.attempt, password);
    },
    [attempt],
  );

  const cancelPassword = useCallback(() => {
    // Invalidates any password check still in flight.
    latest.current++;
    locked.current = null;
    setState((s) => ({ ...s, activity: IDLE }));
  }, []);

  return { state, open, submitPassword, cancelPassword };
}
