// The shell of the annotation dialogs that send something: a modal
// (`ModalShell`) around a form, with Cancel and a submit button.
// Presentational.

import type { FormEvent, ReactNode, RefObject } from 'react';
import { ModalShell } from './ModalShell';

interface ModalFormProps {
  readonly title: string;
  readonly className: string;
  /** The submit button's label. */
  readonly submit: string;
  readonly canSubmit: boolean;
  /** What was sent is being applied: the buttons wait and the dialog cannot be cancelled. */
  readonly busy: boolean;
  /** Why it cannot be used; the dialog stays open. */
  readonly error: string | null;
  readonly errorId: string;
  /** The field that takes the focus. */
  readonly field: RefObject<HTMLElement | null>;
  readonly children: ReactNode;
  readonly onSubmit: () => void;
  readonly onCancel: () => void;
}

export function ModalForm(props: ModalFormProps) {
  const { title, className, submit, canSubmit, busy, error, errorId, field, children, onSubmit, onCancel } = props;

  const send = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!busy && canSubmit) onSubmit();
  };

  return (
    <ModalShell title={title} className={className} busy={busy} field={field} onCancel={onCancel}>
      {(heading) => (
        <form method="dialog" noValidate onSubmit={send}>
          {heading}
          {children}
          {error && (
            <p id={errorId} className="text-dialog__error" role="alert">
              {error}
            </p>
          )}
          <div className="dialog__actions">
            <button type="button" onClick={onCancel} disabled={busy}>
              Cancel
            </button>
            <button type="submit" className="button--primary" disabled={busy || !canSubmit}>
              {submit}
            </button>
          </div>
        </form>
      )}
    </ModalShell>
  );
}
