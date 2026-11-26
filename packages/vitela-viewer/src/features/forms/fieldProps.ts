// What every control of the forms panel is handed. Types only.

import type { FieldValue, FormField, PageRect } from '@vitela/core';

/** Fills a field in; `true` once it holds the value (see `useFormFill`). */
export type FillField = (field: FormField, value: FieldValue) => Promise<boolean>;

export interface FieldProps<F extends FormField = FormField> {
  readonly field: F;
  /** The control's own id, for its label. */
  readonly controlId: string;
  /** The document does not let it be filled in. */
  readonly readOnly: boolean;
  /** The core refused the last value: the id of the text that says why. */
  readonly errorId: string | undefined;
  readonly onFill: FillField;
  /** The focus came to it; `rect` is where on its page (a radio button's own box). */
  readonly onFocus: (field: FormField, rect: PageRect) => void;
  readonly onBlur: () => void;
}

/** The attributes that tie a control to the reason its value was refused. */
export function errorAttributes(errorId: string | undefined) {
  return errorId ? { 'aria-invalid': true, 'aria-describedby': errorId } : {};
}
