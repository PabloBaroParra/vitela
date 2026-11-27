// The one-field dialog Extract pages and Split share, as on the native
// shells: a typed set of pages (or of cuts), a hint that counts the pages,
// and the core's refusal of what was typed, shown with the dialog still open
// and the text still in the field. A document that cannot run the action at
// all says why and offers only Cancel. Presentational: the answers are the
// caller's.

import { useId, useRef } from 'react';
import { ModalShell } from '../annotations/ModalShell';

interface PageRangeDialogProps {
  readonly title: string;
  readonly className: string;
  readonly label: string;
  readonly placeholder: string;
  /** What the field starts with (an Organize selection), else empty. */
  readonly initialText?: string;
  readonly hint: string;
  readonly confirmLabel: string;
  readonly runningLabel: string;
  /** Why the action cannot run on this document, as a sentence; `null` when it can. */
  readonly refusal: string | null;
  /** The refusal of the last text sent, as a sentence. */
  readonly error: string | null;
  /** The text is with the core: nothing can be changed or cancelled. */
  readonly running: boolean;
  readonly onConfirm: (text: string) => void;
  readonly onCancel: () => void;
}

export function PageRangeDialog({
  title,
  className,
  label,
  placeholder,
  initialText = '',
  hint,
  confirmLabel,
  runningLabel,
  refusal,
  error,
  running,
  onConfirm,
  onCancel,
}: PageRangeDialogProps) {
  const field = useRef<HTMLInputElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const hintId = useId();
  const errorId = useId();
  const refused = refusal !== null;

  return (
    <ModalShell title={title} className={`page-range-dialog ${className}`} busy={running} field={refused ? cancel : field} onCancel={onCancel}>
      {(heading) => (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!refused && !running) onConfirm(field.current?.value ?? '');
          }}
        >
          {heading}
          {refused && (
            <p className="page-range-dialog__refusal" role="alert">
              {refusal}
            </p>
          )}
          <label className="page-range-dialog__field">
            {label}
            <input
              ref={field}
              type="text"
              inputMode="text"
              autoComplete="off"
              spellCheck={false}
              placeholder={placeholder}
              defaultValue={initialText}
              disabled={refused}
              readOnly={running}
              aria-invalid={error !== null}
              aria-describedby={error !== null ? `${hintId} ${errorId}` : hintId}
            />
          </label>
          <p id={hintId} className="page-range-dialog__hint">
            {hint}
          </p>
          {error !== null && (
            <p id={errorId} className="page-range-dialog__error" role="alert">
              {error}
            </p>
          )}
          <div className="dialog__actions">
            <button ref={cancel} type="button" onClick={onCancel} disabled={running}>
              Cancel
            </button>
            <button type="submit" className="button--primary" disabled={refused} aria-disabled={running}>
              {running ? runningLabel : confirmLabel}
            </button>
          </div>
        </form>
      )}
    </ModalShell>
  );
}
