// Protect with a password: two passwords, then the protected copy — the
// order of the other shells. Both gates (a document whose permissions
// withhold the change, a signature that would break) are asked as the dialog
// opens, so the dialog can say why before anything runs.
//
// Every password rule is the core's: the dialog sends what was typed and
// shows the core's sentence when it refuses (`InvalidPasswords`), with the
// dialog still open and what was typed still in it. The passwords live only
// in the dialog's own fields and in the one call that protects: never in
// this state, so closing the dialog forgets them.
//
// The native shells reopen the protected file in place; the web cannot (the
// copy is a download, not the file the session was opened from), so the
// open document, its pending edits and its undo history stay exactly as they
// were, and the copy is delivered like a compressed one. Container logic
// only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { INVALID_PASSWORDS, errorKind, errorMessage, type Engine, type ProtectionPasswords } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import type { Delivery } from '../save/delivery';
import type { SaveOutcome } from '../save/outcome';
import { sentence } from '../../ui/text';
import { protectedName } from './protectText';

export type ProtectActivity =
  | { readonly kind: 'idle' }
  /** Asking the gates before the dialog opens. */
  | { readonly kind: 'checking' }
  /** The dialog is open. */
  | {
      readonly kind: 'choosing';
      /** Why it cannot run, as a sentence; `null` when it can. */
      readonly refusal: string | null;
      /** Confirming is the user's yes to breaking the file's signature. */
      readonly signaturesWillBreak: boolean;
      /** The core's refusal of the last passwords sent, as a sentence. */
      readonly error: string | null;
      /** The passwords are with the core: the dialog waits. */
      readonly running: boolean;
    }
  /** The protected copy is on its way to the delivery. */
  | { readonly kind: 'delivering' }
  | { readonly kind: 'done'; readonly outcome: SaveOutcome }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: ProtectActivity = { kind: 'idle' };

/** From the click until the copy is delivered: Protect cannot be started again. */
export function protectBusy(activity: ProtectActivity): boolean {
  return activity.kind === 'checking' || activity.kind === 'choosing' || activity.kind === 'delivering';
}

export type ProtectSession = ReturnType<typeof useProtect>;

export function useProtect(
  engine: Engine,
  document: OpenDocument | null,
  delivery: Delivery,
  /** Settles once the edits the editing screen has asked for so far have ended. */
  settleEdits?: () => Promise<unknown>,
) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<ProtectActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document on screen: an answer for an old document changes nothing. */
  const shown = useRef(generation);
  /** True from the click until the dialog closes: one at a time. */
  const busy = useRef(false);

  // A new document starts clean (adjusting state while rendering).
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
  }
  useEffect(() => {
    shown.current = generation;
    busy.current = false;
  }, [generation]);

  const open = useCallback(async () => {
    if (!document || busy.current) return;
    busy.current = true;
    const asking = document.generation;
    setActivity({ kind: 'checking' });
    let refusal: string | null = null;
    let signaturesWillBreak = false;
    try {
      const refused = await engine.protectionRefusal(document.handle);
      if (refused) refusal = sentence(refused);
      // A failed question refuses rather than guessing "unsigned": a guess
      // could hand over a copy whose signature silently broke.
      else signaturesWillBreak = await engine.protectionInvalidatesSignatures(document.handle);
    } catch (error) {
      refusal = sentence(errorMessage(error));
    }
    if (shown.current !== asking) return;
    setActivity({ kind: 'choosing', refusal, signaturesWillBreak, error: null, running: false });
  }, [engine, document]);

  const cancel = useCallback(() => {
    setActivity((a) => (a.kind === 'choosing' && !a.running ? IDLE : a));
    busy.current = false;
  }, []);

  const protect = useCallback(
    async (passwords: ProtectionPasswords) => {
      if (!document || activity.kind !== 'choosing' || activity.refusal !== null || activity.running) return;
      const { signaturesWillBreak } = activity;
      const running = document.generation;
      const current = () => shown.current === running;
      setActivity({ ...activity, error: null, running: true });
      let bytes: Uint8Array;
      try {
        // The edits asked for before the click go in.
        await settleEdits?.();
        // The dialog showed the signature warning and its button said so:
        // confirming it is the acknowledgement, never a second prompt.
        bytes = await engine.protectToBytes(document.handle, passwords, signaturesWillBreak);
      } catch (error) {
        if (!current()) return;
        if (errorKind(error) === INVALID_PASSWORDS) {
          // The core's rule, in its words: the dialog stays, so they can be corrected.
          setActivity({ ...activity, error: sentence(errorMessage(error)), running: false });
        } else {
          setActivity({ kind: 'failed', message: errorMessage(error) });
          busy.current = false;
        }
        return;
      }
      if (!current()) return;
      // The bytes are ready: the dialog (and the passwords in its fields) goes.
      setActivity({ kind: 'delivering' });
      try {
        const outcome = await delivery.deliver({ bytes, name: protectedName(document.name), destination: { kind: 'newFile' } });
        if (!current()) return;
        // A new file is never compared with the disk, so this cannot happen; say nothing was written if it does.
        setActivity({ kind: 'done', outcome: outcome.how === 'changedOnDisk' ? { how: 'cancelled' } : outcome });
      } catch (error) {
        if (current()) setActivity({ kind: 'failed', message: errorMessage(error) });
      } finally {
        if (current()) busy.current = false;
      }
    },
    [engine, document, delivery, settleEdits, activity],
  );

  return { activity, open, cancel, protect };
}
