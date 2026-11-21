// The shell of the save questions: a native modal <dialog> in the warning
// style (icon badge, title, body, actions). A modal <dialog> keeps focus
// inside it and makes the rest of the page inert; Escape fires `cancel`.
// Presentational: the question and its answers come from the caller.

import { useEffect, useId, useRef, type ReactNode } from 'react';

interface WarningDialogProps {
  readonly icon: ReactNode;
  readonly title: string;
  /** The explanation, as paragraphs. */
  readonly children: ReactNode;
  /** The buttons; the one with `autoFocus` takes the focus. */
  readonly actions: ReactNode;
  readonly onCancel: () => void;
}

export function WarningDialog({ icon, title, children, actions, onCancel }: WarningDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const bodyId = useId();

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (!element.open) element.showModal();
    return () => element.close();
  }, []);

  return (
    <dialog
      ref={dialog}
      className="dialog dialog--warning"
      aria-labelledby={titleId}
      aria-describedby={bodyId}
      onCancel={(event) => {
        // The parent unmounts the dialog; do not let the browser close it first.
        event.preventDefault();
        onCancel();
      }}
    >
      <div className="dialog__icon" aria-hidden="true">
        {icon}
      </div>
      <h2 id={titleId} className="dialog__title">
        {title}
      </h2>
      <div id={bodyId} className="dialog__body">
        {children}
      </div>
      <div className="dialog__actions">{actions}</div>
    </dialog>
  );
}
