// The shell of the editor's small dialogs: a native modal <dialog> (focus
// stays inside, the page behind is inert, Escape fires `cancel`) under a
// title. It stays open for as long as it is mounted, even if the browser
// closes it on its own, and gives the focus back to what had it.
// Presentational.

import { useId, useLayoutEffect, useRef, type ReactNode, type RefObject } from 'react';

interface ModalShellProps {
  readonly title: string;
  readonly className: string;
  /** What was sent is being applied: the dialog cannot be cancelled. */
  readonly busy?: boolean;
  /** The field that takes the focus. */
  readonly field: RefObject<HTMLElement | null>;
  /** The content, around the heading it is handed: a form when there is something to send. */
  readonly children: (heading: ReactNode) => ReactNode;
  readonly onCancel: () => void;
}

export function ModalShell({ title, className, busy = false, field, children, onCancel }: ModalShellProps) {
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
      {children(
        <h2 id={titleId} className="dialog__title">
          {title}
        </h2>,
      )}
    </dialog>
  );
}
