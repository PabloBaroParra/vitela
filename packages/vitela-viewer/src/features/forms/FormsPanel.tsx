// The document's form fields by page, each with the control its kind takes:
// filled in from here, or only read where the document does not allow it.
// The controls follow the list, so Tab goes from field to field. Presentational.

import { useId, useRef } from 'react';
import type { FormField, PageRect } from '@vitela/core';
import { focusing } from '../annotations/focusing';
import { useKeepFocus } from '../annotations/useKeepFocus';
import { CheckboxInput, DropdownSelect, RadioInputs } from './ChoiceInputs';
import type { FieldProps, FillField } from './fieldProps';
import { fieldsByPage } from './fieldValues';
import { EditableDropdownInput, TextFieldInput } from './TextFieldInput';
import type { FormFieldsState } from './useFormFields';

interface FormsPanelProps {
  readonly id: string;
  readonly state: FormFieldsState;
  /** The fields may be filled in. */
  readonly allowed: boolean;
  /** Why the core refused the last value of a field, by field id. */
  readonly errors: ReadonlyMap<number, string>;
  readonly onFill: FillField;
  readonly onFocusField: (field: FormField, rect: PageRect) => void;
  readonly onBlurField: () => void;
  readonly onRetry: () => void;
  readonly onClose: () => void;
}

const READ_ONLY = 'This document does not allow its form fields to be filled in.';

export function FormsPanel(props: FormsPanelProps) {
  const { id, state, allowed, onRetry, onClose } = props;
  const panel = useRef<HTMLElement>(null);
  const focusPanel = () => panel.current?.focus({ preventScroll: true });
  return (
    <aside
      ref={panel}
      id={id}
      className="forms-panel"
      aria-label="Form fields"
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key !== 'Escape' || event.defaultPrevented) return;
        event.preventDefault();
        onClose();
      }}
    >
      <div className="forms-panel__head">
        <h2 className="forms-panel__title">Form fields</h2>
        <button type="button" aria-label="Close form fields" onClick={onClose}>
          ×
        </button>
      </div>
      {state.status === 'loading' && (
        <p className="forms-panel__note" role="status">
          Loading form fields…
        </p>
      )}
      {state.status === 'failed' && <Failure message={state.message} onRetry={onRetry} onFocusLost={focusPanel} />}
      {state.status === 'ready' && state.fields.length === 0 && <p className="forms-panel__note">This document has no form fields.</p>}
      {!allowed && state.fields.length > 0 && <p className="forms-panel__note">{READ_ONLY}</p>}
      {fieldsByPage(state.fields).map(([pageIndex, onPage]) => (
        <section key={pageIndex}>
          <h3 className="forms-panel__page">{`Page ${pageIndex + 1}`}</h3>
          <ul className="forms-panel__list">
            {onPage.map((field) => (
              <FieldRow key={field.id} {...props} field={field} onFocusLost={focusPanel} />
            ))}
          </ul>
        </section>
      ))}
    </aside>
  );
}

interface FailureProps {
  readonly message: string;
  readonly onRetry: () => void;
  readonly onFocusLost: () => void;
}

function Failure({ message, onRetry, onFocusLost }: FailureProps) {
  const block = useRef<HTMLDivElement>(null);
  // "Try again" goes away with the failure: the focus must not drop on <body>.
  useKeepFocus(block, onFocusLost);
  return (
    <div ref={block} className="forms-panel__note">
      <p className="forms-panel__error" role="alert">{`Could not list the form fields: ${message}`}</p>
      <button type="button" onClick={focusing(onRetry)}>
        Try again
      </button>
    </div>
  );
}

interface FieldRowProps extends FormsPanelProps {
  readonly field: FormField;
  readonly onFocusLost: () => void;
}

function FieldRow({ field, allowed, errors, onFill, onFocusField, onBlurField, onFocusLost }: FieldRowProps) {
  const row = useRef<HTMLLIElement>(null);
  // An undo that takes the field's page away must not drop the focus on <body>.
  useKeepFocus(row, onFocusLost);
  const controlId = useId();
  const labelId = useId();
  const error = errors.get(field.id);
  const errorId = error ? `${controlId}-error` : undefined;
  const common: Omit<FieldProps, 'field'> = { controlId, readOnly: !allowed, errorId, onFill, onFocus: onFocusField, onBlur: onBlurField };
  return (
    <li ref={row} className="form-field" data-kind={field.kind}>
      {field.kind === 'radio' ? (
        <>
          <span id={labelId} className="form-field__label">
            {field.name}
          </span>
          <RadioInputs {...common} field={field} labelId={labelId} />
        </>
      ) : field.kind === 'checkbox' ? (
        <span className="form-field__inline">
          <CheckboxInput {...common} field={field} />
          <label htmlFor={controlId} className="form-field__label">
            {field.name}
          </label>
        </span>
      ) : (
        <>
          <label htmlFor={controlId} className="form-field__label">
            {field.name}
          </label>
          {field.kind === 'text' ? (
            <TextFieldInput {...common} field={field} />
          ) : field.editable ? (
            <EditableDropdownInput {...common} field={field} />
          ) : (
            <DropdownSelect {...common} field={field} />
          )}
        </>
      )}
      {error && (
        <p id={errorId} className="form-field__error" role="alert">
          {error}
        </p>
      )}
    </li>
  );
}
