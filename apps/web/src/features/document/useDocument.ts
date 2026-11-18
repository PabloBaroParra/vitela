// Opening a document: reads the picked file, hands its bytes to the engine,
// asks for a password when the core says one is needed, and tracks what the
// UI should show. Container logic only, no markup.

import { useCallback, useRef, useState } from 'react';
import { PASSWORD_REQUIRED, WRONG_PASSWORD, errorKind, errorMessage } from '@vitela/core';
import type { DocumentHandle, DocumentInfo, EditedDocument, Engine } from '@vitela/core';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { recordFileStamp } from '../../platform/fileStamp';

export interface OpenDocument {
  readonly name: string;
  readonly info: DocumentInfo;
  /** The engine's handle: every render of this document names it. */
  readonly handle: DocumentHandle;
  /** Bumps on every successful open, so views reset for a same-named file. */
  readonly generation: number;
  /**
   * The handle to save back over the opened file, when the browser gave one
   * (Chromium's picker or drop). Without it, saving downloads.
   */
  readonly fileHandle?: WritableFileHandle;
  /**
   * Where the edit history stands, as the last edit, undo or redo reported it
   * (unset until the first one: a freshly opened document has no history).
   */
  readonly canUndo?: boolean;
  readonly canRedo?: boolean;
  /**
   * The file each PDF added to this document came from, by the block source
   * id the import reported (the engine never sees a file name). Unset until
   * the first import.
   */
  readonly sourceNames?: ReadonlyMap<number, string>;
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
   * The document on screen. A failed or cancelled open leaves it in place;
   * it is closed in the engine only once a new one has opened.
   */
  readonly document: OpenDocument | null;
  readonly activity: Activity;
}

const IDLE: Activity = { kind: 'idle' };

export function useDocument(engine: Engine) {
  const [state, setState] = useState<DocumentState>({ document: null, activity: IDLE });
  const latest = useRef(0);
  /** The file waiting for a password; `null` when no prompt is open. */
  const locked = useRef<{ file: File; fileHandle?: WritableFileHandle; attempt: number } | null>(null);
  /** The handle of the document on screen, the one engine document kept. */
  const shown = useRef<DocumentHandle | null>(null);
  const release = useCallback(
    // Nothing to recover if a close fails: the handle is never used again.
    (handle: DocumentHandle) => void engine.close(handle).catch(() => {}),
    [engine],
  );

  const attempt = useCallback(
    (file: File, fileHandle: WritableFileHandle | undefined, attemptId: number, password: string | undefined) => {
      const current = () => attemptId === latest.current;
      // Read afresh each time: the engine takes the bytes by transfer.
      file
        .arrayBuffer()
        .then((buffer) => engine.open(new Uint8Array(buffer), password))
        .then(
          ({ handle, info }) => {
            // Superseded or cancelled while the engine was opening it: the
            // engine still holds it, so hand it back rather than drop it.
            if (!current()) return release(handle);
            locked.current = null;
            const previous = shown.current;
            shown.current = handle;
            setState({ document: { name: file.name, info, handle, generation: attemptId, fileHandle }, activity: IDLE });
            if (previous !== null) release(previous);
          },
          (error: unknown) => {
            if (!current()) return;
            const kind = errorKind(error);
            if (kind === PASSWORD_REQUIRED || kind === WRONG_PASSWORD) {
              locked.current = { file, fileHandle, attempt: attemptId };
              const wrong = password !== undefined;
              setState((s) => ({ ...s, activity: { kind: 'password', name: file.name, wrong, checking: false } }));
            } else {
              locked.current = null;
              setState((s) => ({ ...s, activity: { kind: 'failed', name: file.name, message: errorMessage(error) } }));
            }
          },
        );
    },
    [engine, release],
  );

  const open = useCallback(
    (file: File, fileHandle?: WritableFileHandle) => {
      const attemptId = ++latest.current;
      locked.current = null;
      // The file as the user opened it, to notice a change made behind our back before a save.
      if (fileHandle) recordFileStamp(fileHandle);
      setState((s) => ({ ...s, activity: { kind: 'opening', name: file.name } }));
      attempt(file, fileHandle, attemptId, undefined);
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
      attempt(pending.file, pending.fileHandle, pending.attempt, password);
    },
    [attempt],
  );

  const cancelPassword = useCallback(() => {
    // Invalidates any password check still in flight.
    latest.current++;
    locked.current = null;
    setState((s) => ({ ...s, activity: IDLE }));
  }, []);

  /**
   * The document after an edit, undo or redo (`info` changes with it: page
   * count, sizes, ids). An answer for a document no longer on screen is
   * dropped; the result says whether it was taken.
   */
  const revise = useCallback((handle: DocumentHandle, edited: EditedDocument): boolean => {
    if (shown.current !== handle) return false;
    setState((s) =>
      s.document?.handle === handle
        ? { ...s, document: { ...s.document, info: edited.info, canUndo: edited.canUndo, canRedo: edited.canRedo } }
        : s,
    );
    return true;
  }, []);

  /** Records which file each imported PDF came from, for the document still on screen. */
  const nameSources = useCallback((handle: DocumentHandle, names: readonly (readonly [number, string])[]) => {
    setState((s) =>
      s.document?.handle === handle
        ? { ...s, document: { ...s.document, sourceNames: new Map([...(s.document.sourceNames ?? []), ...names]) } }
        : s,
    );
  }, []);

  return { state, open, submitPassword, cancelPassword, revise, nameSources };
}
