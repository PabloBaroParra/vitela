// The "Move to" and "Resize" dialogs' shell: two typed lengths in points. A
// modal form; what the lengths mean, and whether they can be used, is the
// caller's (`editDialogs.ts`). Presentational.

import { useId, useRef, type ReactNode, type RefObject } from 'react';
import { ModalForm } from './ModalForm';

export interface LengthField {
  readonly label: string;
  readonly value: string;
  readonly onChange: (value: string) => void;
}

interface LengthsDialogProps {
  readonly title: string;
  readonly submit: string;
  readonly fields: readonly [LengthField, LengthField];
  readonly busy: boolean;
  readonly error: string | null;
  /** Shown under the fields. */
  readonly children?: ReactNode;
  readonly onSubmit: () => void;
  readonly onCancel: () => void;
}

export function LengthsDialog({ title, submit, fields, busy, error, children, onSubmit, onCancel }: LengthsDialogProps) {
  const first = useRef<HTMLInputElement>(null);
  const errorId = useId();
  const input = ({ label, value, onChange }: LengthField, ref?: RefObject<HTMLInputElement | null>) => (
    <label key={label} className="lengths-dialog__field">
      {`${label} (pt)`}
      <input
        ref={ref}
        type="text"
        inputMode="decimal"
        autoComplete="off"
        value={value}
        aria-invalid={!!error}
        aria-describedby={error ? errorId : undefined}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
  return (
    <ModalForm
      title={title}
      className="lengths-dialog"
      submit={submit}
      canSubmit
      busy={busy}
      error={error}
      errorId={errorId}
      field={first}
      onSubmit={onSubmit}
      onCancel={onCancel}
    >
      <div className="lengths-dialog__fields">
        {input(fields[0], first)}
        {input(fields[1])}
      </div>
      {children}
    </ModalForm>
  );
}
