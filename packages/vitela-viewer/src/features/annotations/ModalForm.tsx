// The shell of the annotation dialogs: a native modal <dialog> (focus stays
// inside, the page behind is inert, Escape fires `cancel`) around a form,
// with Cancel and a submit button. It stays open for as long as it is
// mounted, even if the browser closes it on its own, and gives the focus
// back to what had it. Presentational.

import { useId, useLayoutEffect, useRef, type FormEvent, type ReactNode, type RefObject } from 'react';

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
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  // A layout effect: its cleanup runs while the dialog is still on the page.
  useLayoutEffect(() => {
    const element = dialog.current;
    if (!element) return;
    const opener = element.ownerDocument.activeElement;
    if (!element.open) element.showModal();
    field.current?.focus();
    return () => {
      element.close();
      if (opener instanceof HTMLElement && opener.isConnected && !element.contains(opener)) {
        opener.focus({ preventScroll: true });
      }
    };
  }, [field]);

  const send = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!busy && canSubmit) onSubmit();
  };

  return (
    <dialog
      ref={dialog}
      className={`dialog ${className}`}
      aria-labelledby={titleId}
      onCancel={(event) => {
        // The parent unmounts the dialog; do not let the browser close it first.
        // While busy, the edit is already queued: stay open until it settles.
        event.preventDefault();
        if (!busy) onCancel();
      }}
      onClose={() => {
        // Chromium lets a second Escape in a row close a modal whose `cancel`
        // was prevented. Shown means open: while mounted (the parent has not
        // dropped it) it opens again, so a refusal still has somewhere to
        // show. The close of an unmounting one is told once it is detached.
        const element = dialog.current;
        if (element?.isConnected && !element.open) element.showModal();
      }}
    >
      <form method="dialog" noValidate onSubmit={send}>
        <h2 id={titleId} className="dialog__title">
          {title}
        </h2>
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
    </dialog>
  );
}
