// Document properties: the six text keys of `/Info`, one box each, with
// Android's labels and order. A document that withholds modification shows
// them read-only with the core's sentence and offers only Close.
// Presentational: the draft and the answers are the caller's.

import { useId, useRef } from 'react';
import { ModalShell } from '../annotations/ModalShell';
import { PROPERTY_FIELDS, type PropertiesDraft } from './propertiesText';

interface PropertiesDialogProps {
  readonly draft: PropertiesDraft;
  /** Why the properties are read-only, as a sentence; `null` when they can be changed. */
  readonly refusal: string | null;
  /** Why the last apply did not go in, as a sentence. */
  readonly error: string | null;
  /** The change is with the core: nothing can be changed or cancelled. */
  readonly running: boolean;
  readonly onEdit: (draft: PropertiesDraft) => void;
  readonly onApply: () => void;
  readonly onCancel: () => void;
}

export function PropertiesDialog({ draft, refusal, error, running, onEdit, onApply, onCancel }: PropertiesDialogProps) {
  const first = useRef<HTMLInputElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const errorId = useId();
  const readOnly = refusal !== null;

  return (
    <ModalShell
      title="Document properties"
      className="properties-dialog"
      busy={running}
      field={readOnly ? close : first}
      onCancel={onCancel}
    >
      {(heading) => (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!readOnly && !running) onApply();
          }}
        >
          {heading}
          {readOnly && (
            <p className="properties-dialog__refusal" role="alert">
              {refusal}
            </p>
          )}
          {PROPERTY_FIELDS.map(({ key, label }, index) => (
            <label key={key} className="properties-dialog__field">
              {label}
              <input
                ref={index === 0 ? first : undefined}
                type="text"
                value={draft[key]}
                readOnly={readOnly || running}
                spellCheck={false}
                aria-invalid={error !== null}
                aria-describedby={error !== null ? errorId : undefined}
                onChange={(event) => onEdit({ ...draft, [key]: event.target.value })}
              />
            </label>
          ))}
          {error !== null && (
            <p id={errorId} className="properties-dialog__error" role="alert">
              {error}
            </p>
          )}
          <div className="dialog__actions">
            <button ref={close} type="button" onClick={onCancel} disabled={running}>
              {readOnly ? 'Close' : 'Cancel'}
            </button>
            {!readOnly && (
              <button type="submit" className="button--primary" aria-disabled={running}>
                {running ? 'Applying…' : 'Apply'}
              </button>
            )}
          </div>
        </form>
      )}
    </ModalShell>
  );
}
