// A text field, and a dropdown that takes typed values: filled in when the
// focus leaves it, or on Enter in a one-line field. Escape puts back what the
// document holds (and, with nothing to put back, goes on to close the panel).
// Presentational.

import type { KeyboardEvent } from 'react';
import type { FormField } from '@vitela/core';
import { errorAttributes, type FieldProps } from './fieldProps';
import { useDraft } from './useDraft';

type TextField = Extract<FormField, { kind: 'text' }>;
type Dropdown = Extract<FormField, { kind: 'dropdown' }>;

export function TextFieldInput(props: FieldProps<TextField>) {
  const { field, onFill } = props;
  return (
    <DraftInput
      {...props}
      current={field.value}
      multiline={field.multiline}
      maxLength={field.maxLength ?? undefined}
      commit={(text) => onFill(field, { kind: 'text', text })}
    />
  );
}

/** An editable dropdown: its options are suggestions; an empty value chooses nothing. */
export function EditableDropdownInput(props: FieldProps<Dropdown>) {
  const { field, controlId, onFill } = props;
  const listId = `${controlId}-options`;
  return (
    <>
      <DraftInput
        {...props}
        current={field.selected ?? ''}
        multiline={false}
        list={listId}
        commit={(text) => onFill(field, { kind: 'choice', option: text === '' ? null : text })}
      />
      <datalist id={listId}>
        {field.options.map((option) => (
          <option key={option} value={option} />
        ))}
      </datalist>
    </>
  );
}

interface DraftInputProps extends FieldProps {
  readonly current: string;
  readonly multiline: boolean;
  readonly maxLength?: number;
  readonly list?: string;
  readonly commit: (text: string) => Promise<boolean>;
}

function DraftInput({
  field,
  controlId,
  readOnly,
  errorId,
  onFocus,
  onBlur,
  current,
  multiline,
  maxLength,
  list,
  commit,
}: DraftInputProps) {
  const draft = useDraft(current, readOnly, commit);
  const common = {
    id: controlId,
    className: 'form-field__input',
    value: draft.value,
    readOnly,
    maxLength,
    ...errorAttributes(errorId),
    onChange: (event: { currentTarget: { value: string } }) => draft.change(event.currentTarget.value),
    onFocus: () => onFocus(field, field.rect),
    onBlur: () => {
      void draft.send();
      onBlur();
    },
    onKeyDown: (event: KeyboardEvent) => {
      if (event.key === 'Escape' && draft.revert()) event.preventDefault();
      if (event.key === 'Enter' && !multiline) {
        event.preventDefault();
        void draft.send();
      }
    },
  };
  return multiline ? <textarea {...common} rows={3} /> : <input {...common} type="text" list={list} />;
}
