// Protect with a password: both PDF password roles, as on the other shells
// (one to open the file, one that lifts its permissions). The passwords stay
// in this dialog's own fields — never in React state — and leave it only
// through `onProtect`; the fields are emptied when the dialog goes, so
// closing it forgets them. A signed file says so here and the button reads
// "Protect anyway", so confirming is the consent; a document that cannot be
// protected says why and offers only Cancel. Presentational: the answers
// are the caller's.

import { useId, useLayoutEffect, useRef } from 'react';
import type { ProtectionPasswords } from '@vitela/core';
import { ModalShell } from '../annotations/ModalShell';

interface ProtectDialogProps {
  /** Why the document cannot be protected, as a sentence; `null` when it can. */
  readonly refusal: string | null;
  readonly signaturesWillBreak: boolean;
  /** The core's refusal of the last passwords sent, as a sentence. */
  readonly error: string | null;
  /** The passwords are with the core: nothing can be changed or cancelled. */
  readonly running: boolean;
  readonly onProtect: (passwords: ProtectionPasswords) => void;
  readonly onCancel: () => void;
}

export function ProtectDialog({ refusal, signaturesWillBreak, error, running, onProtect, onCancel }: ProtectDialogProps) {
  const openField = useRef<HTMLInputElement>(null);
  const permissionsField = useRef<HTMLInputElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const errorId = useId();
  const refused = refusal !== null;

  // What was typed does not outlive the dialog.
  useLayoutEffect(() => {
    const fields = [openField.current, permissionsField.current];
    return () => {
      for (const field of fields) if (field) field.value = '';
    };
  }, []);

  const field = (label: string, ref: typeof openField) => (
    <label className="protect-dialog__field">
      {label}
      <input
        ref={ref}
        type="password"
        autoComplete="new-password"
        spellCheck={false}
        disabled={refused}
        readOnly={running}
        aria-invalid={error !== null}
        aria-describedby={error !== null ? errorId : undefined}
      />
    </label>
  );

  return (
    <ModalShell
      title="Protect with a password"
      className="protect-dialog"
      busy={running}
      field={refused ? cancel : openField}
      onCancel={onCancel}
    >
      {(heading) => (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (refused || running) return;
            onProtect({
              openPassword: openField.current?.value ?? '',
              permissionsPassword: permissionsField.current?.value ?? '',
            });
          }}
        >
          {heading}
          {refused && (
            <p className="protect-dialog__refusal" role="alert">
              {refusal}
            </p>
          )}
          {field('Password to open the document', openField)}
          {field('Permissions password', permissionsField)}
          <p className="protect-dialog__detail">
            Use different passwords. The permissions password controls changes after the document is opened. Writes a
            protected copy; the open document stays as it is.
          </p>
          {signaturesWillBreak && (
            <p className="protect-dialog__signed">
              This document is signed. Applying password protection rewrites the file, so its existing digital signature
              will no longer verify.
            </p>
          )}
          {error !== null && (
            <p id={errorId} className="protect-dialog__error" role="alert">
              {error}
            </p>
          )}
          <div className="dialog__actions">
            <button ref={cancel} type="button" onClick={onCancel} disabled={running}>
              Cancel
            </button>
            <button type="submit" className="button--primary" disabled={refused} aria-disabled={running}>
              {running ? 'Protecting…' : signaturesWillBreak ? 'Protect anyway' : 'Protect'}
            </button>
          </div>
        </form>
      )}
    </ModalShell>
  );
}
