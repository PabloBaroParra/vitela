// A checkbox, a radio group and a dropdown of fixed options: each is filled
// in as soon as it changes (there is no typing to wait for). The choice shows
// at once and, once the fill has ended, what the document holds — the old
// value again if the core refused it. Presentational.

import { useId, useState } from 'react';
import type { FormField } from '@vitela/core';
import { errorAttributes, type FieldProps } from './fieldProps';

type Checkbox = Extract<FormField, { kind: 'checkbox' }>;
type Radio = Extract<FormField, { kind: 'radio' }>;
type Dropdown = Extract<FormField, { kind: 'dropdown' }>;

/** A value chosen and not yet filled in, shown in place of the document's. */
function usePending<T>(current: T) {
  const [pending, setPending] = useState<{ readonly value: T } | null>(null);
  const choose = async (value: T, fill: () => Promise<boolean>) => {
    const token = { value };
    setPending(token);
    try {
      await fill();
    } finally {
      setPending((shown) => (shown === token ? null : shown));
    }
  };
  return { shown: pending ? pending.value : current, choose };
}

export function CheckboxInput({ field, controlId, readOnly, errorId, onFill, onFocus, onBlur }: FieldProps<Checkbox>) {
  const { shown, choose } = usePending(field.checked);
  return (
    <input
      id={controlId}
      type="checkbox"
      className="form-field__check"
      checked={shown}
      disabled={readOnly}
      {...errorAttributes(errorId)}
      onChange={(event) => {
        const checked = event.currentTarget.checked;
        void choose(checked, () => onFill(field, { kind: 'checked', checked }));
      }}
      onFocus={() => onFocus(field, field.rect)}
      onBlur={onBlur}
    />
  );
}

/** Labelled by the `legend` the panel gives it (`labelId`). */
export function RadioInputs({
  field,
  readOnly,
  errorId,
  onFill,
  onFocus,
  onBlur,
  labelId,
}: FieldProps<Radio> & { readonly labelId: string }) {
  const group = useId();
  const { shown, choose } = usePending(field.selected);
  return (
    <div className="form-field__radios" role="radiogroup" aria-labelledby={labelId} {...errorAttributes(errorId)}>
      {field.buttons.map((button) => (
        <label key={button.exportValue} className="form-field__radio">
          <input
            type="radio"
            name={group}
            value={button.exportValue}
            checked={shown === button.exportValue}
            disabled={readOnly}
            onChange={() => void choose(button.exportValue, () => onFill(field, { kind: 'choice', option: button.exportValue }))}
            onFocus={() => onFocus(field, button.rect)}
            onBlur={onBlur}
          />
          {button.exportValue}
        </label>
      ))}
    </div>
  );
}

/** The empty option chooses nothing. */
export function DropdownSelect({ field, controlId, readOnly, errorId, onFill, onFocus, onBlur }: FieldProps<Dropdown>) {
  const { shown, choose } = usePending(field.selected);
  return (
    <select
      id={controlId}
      className="form-field__input"
      value={shown ?? ''}
      disabled={readOnly}
      {...errorAttributes(errorId)}
      onChange={(event) => {
        const option = event.currentTarget.value === '' ? null : event.currentTarget.value;
        void choose(option, () => onFill(field, { kind: 'choice', option }));
      }}
      onFocus={() => onFocus(field, field.rect)}
      onBlur={onBlur}
    >
      <option value="">(None)</option>
      {field.options.map((option) => (
        <option key={option} value={option}>
          {option}
        </option>
      ))}
    </select>
  );
}
