// Asks for the text of a note or a text box. A modal form whose text area
// takes the focus. Presentational: the caller adds (or retypes) the
// annotation and keeps the dialog open with `error` when the core refuses
// the text (a character Helvetica cannot show, for a text box).

import { useId, useRef, useState, type ReactNode } from 'react';
import { isBlank } from './drawing';
import { ModalForm } from './ModalForm';

interface TextDialogProps {
  readonly title: string;
  /** The text area's label. */
  readonly label: string;
  /** The submit button's label. */
  readonly submit: string;
  /** The text the dialog opens with. */
  readonly initialText?: string;
  /** The text was sent: the buttons wait and the dialog cannot be cancelled. */
  readonly busy: boolean;
  /** Why the text cannot be used, shown under it; the dialog stays open. */
  readonly error: string | null;
  /** Shown under the text area (a text box's layout preview). */
  readonly children?: ReactNode;
  readonly onChange?: (text: string) => void;
  readonly onSubmit: (text: string) => void;
  readonly onCancel: () => void;
}

export function TextDialog(props: TextDialogProps) {
  const { title, label, submit, initialText = '', busy, error, children, onChange, onSubmit, onCancel } = props;
  const field = useRef<HTMLTextAreaElement>(null);
  const errorId = useId();
  const [text, setText] = useState(initialText);

  return (
    <ModalForm
      title={title}
      className="text-dialog"
      submit={submit}
      canSubmit={!isBlank(text)}
      busy={busy}
      error={error}
      errorId={errorId}
      field={field}
      onSubmit={() => onSubmit(text)}
      onCancel={onCancel}
    >
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
            // Ctrl/⌘+Enter submits; Enter alone is a new line.
            if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
              event.preventDefault();
              event.currentTarget.form?.requestSubmit();
            }
          }}
        />
      </label>
      {children}
    </ModalForm>
  );
}
