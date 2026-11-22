// Asks for the password of an encrypted PDF — the one being opened, or one
// being added to it (`action`). Presentational: it reports the password or
// the cancel. A native modal <dialog> keeps focus inside it and
// makes the rest of the page inert; Escape fires `cancel`.

import { useEffect, useId, useRef, type FormEvent } from 'react';

interface PasswordDialogProps {
  readonly name: string;
  /** The last password tried did not open the document. */
  readonly wrong: boolean;
  readonly checking: boolean;
  readonly onSubmit: (password: string) => void;
  readonly onCancel: () => void;
  /** What the password is for: opening the PDF (the default) or adding its pages. */
  readonly action?: 'open' | 'add';
}

const WORDING = {
  open: { purpose: 'open it', submit: 'Open', checking: 'Opening…' },
  add: { purpose: 'add its pages', submit: 'Add', checking: 'Adding…' },
} as const;

export function PasswordDialog({ name, wrong, checking, onSubmit, onCancel, action = 'open' }: PasswordDialogProps) {
  const wording = WORDING[action];
  const dialog = useRef<HTMLDialogElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const errorId = useId();

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (!element.open) element.showModal();
    return () => element.close();
  }, []);

  // After a wrong password, select it so typing replaces it.
  useEffect(() => {
    if (wrong && !checking) input.current?.select();
  }, [wrong, checking]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!checking) onSubmit(input.current?.value ?? '');
  };

  return (
    <dialog
      ref={dialog}
      className="password-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        // The parent unmounts the dialog; do not let the browser close it first.
        event.preventDefault();
        onCancel();
      }}
    >
      <form method="dialog" onSubmit={submit}>
        <h2 id={titleId} className="password-dialog__title">
          Password required
        </h2>
        <p>{`${name} is protected. Enter its password to ${wording.purpose}.`}</p>
        <label className="password-dialog__field">
          Password
          <input
            ref={input}
            type="password"
            name="password"
            autoComplete="off"
            autoFocus
            aria-invalid={wrong}
            aria-describedby={wrong ? errorId : undefined}
          />
        </label>
        {wrong && !checking && (
          <p id={errorId} className="password-dialog__error" role="alert">
            Wrong password. Try again.
          </p>
        )}
        <div className="password-dialog__actions">
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit" className="button--primary" disabled={checking}>
            {checking ? wording.checking : wording.submit}
          </button>
        </div>
      </form>
    </dialog>
  );
}
