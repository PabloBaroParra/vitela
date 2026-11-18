// Asks for the text of a note or a text box placed on the page. A native
// modal <dialog> (focus stays inside, the page behind is inert, Escape fires
// `cancel`) whose text area takes the focus. It stays open for as long as it
// is mounted, even if the browser closes it on its own. Presentational: the caller
// adds the annotation and keeps the dialog open with `error` when the core
// refuses the text (a character Helvetica cannot show, for a text box).

import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { isBlank } from './drawing';

interface TextDialogProps {
  readonly title: string;
  /** The text area's label. */
  readonly label: string;
  /** The submit button's label. */
  readonly submit: string;
  /** The annotation is being added: the buttons wait and the dialog cannot be cancelled. */
  readonly busy: boolean;
  /** Why the text cannot be used, shown under it; the dialog stays open. */
  readonly error: string | null;
  /** Shown under the text area (a text box's layout preview). */
  readonly children?: ReactNode;
  readonly onChange?: (text: string) => void;
  readonly onSubmit: (text: string) => void;
  readonly onCancel: () => void;
}

export function TextDialog({ title, label, submit, busy, error, children, onChange, onSubmit, onCancel }: TextDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const titleId = useId();
  const errorId = useId();
  const [text, setText] = useState('');

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (!element.open) element.showModal();
    field.current?.focus();
    return () => element.close();
  }, []);

  const send = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!busy && !isBlank(text)) onSubmit(text);
  };

  return (
    <dialog
      ref={dialog}
      className="dialog text-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        // The parent unmounts the dialog; do not let the browser close it first.
        // While adding, the edit is already queued: stay open until it settles.
        event.preventDefault();
        if (!busy) onCancel();
      }}
      onClose={() => {
        // Chromium lets a second Escape in a row close a modal whose `cancel`
        // was prevented. Shown means open: while mounted (the parent has not
        // dropped the draft) it opens again, so a refusal still has somewhere
        // to show. Unmounting closes it from a detached node, out of reach.
        const element = dialog.current;
        if (element?.isConnected && !element.open) element.showModal();
      }}
    >
      <form method="dialog" onSubmit={send}>
        <h2 id={titleId} className="dialog__title">
          {title}
        </h2>
        <label className="text-dialog__field">
          {label}
          <textarea
            ref={field}
            name="text"
            rows={4}
            autoFocus
            value={text}
            aria-invalid={!!error}
            aria-describedby={error ? errorId : undefined}
            onChange={(event) => {
              setText(event.target.value);
              onChange?.(event.target.value);
            }}
            onKeyDown={(event) => {
              // Ctrl/⌘+Enter adds; Enter alone is a new line.
              if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
                event.preventDefault();
                event.currentTarget.form?.requestSubmit();
              }
            }}
          />
        </label>
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
          <button type="submit" className="button--primary" disabled={busy || isBlank(text)}>
            {submit}
          </button>
        </div>
      </form>
    </dialog>
  );
}
