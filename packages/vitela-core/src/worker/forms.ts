// Form fields across the worker boundary: the `FormEdit` becomes one of
// core/pdf-wasm's fill `EditCommand`s, and the core's flat `FormFieldEntry`
// (one class for every kind, see core/pdf-wasm/src/dto/form.rs) becomes the
// typed union in core/types.ts.
//
// Like annotations, a field's box is turned from the core's PDF space
// (bottom-left origin) onto the page as drawn by the core's own transform
// (`PageFrame`). A fill carries no geometry, so it crosses as is and every
// refusal is the core's, in its order: the permission, then the id, then the
// value.
//
// The decoder never trusts a field the kind does not have: wasm-bindgen types
// an absent `Option` as present in its `.d.ts` (T-227), and an entry of a kind
// this package does not know is skipped rather than guessed at.

import { EngineError, FORM_FIELD_NOT_FOUND } from '../core/errors.js';
import type { FormEdit, FormField, RadioButton } from '../core/types.js';
import type { PageFrame } from './annotations.js';
import type { Owned } from './pageText.js';

/** core/pdf-wasm's fill `EditCommand` constructors, one per kind of value. */
export interface CoreFormEdits {
  setFieldText(id: number, text: string): Owned;
  setFieldChecked(id: number, checked: boolean): Owned;
  /** `undefined` (wasm-bindgen's `None`) chooses nothing. */
  setFieldChoice(id: number, option: string | undefined): Owned;
}

/** core/pdf-wasm's `FormFieldEntry`: every field for every kind, empty when the kind has none. */
export interface CoreFormField extends Owned {
  readonly id: number;
  readonly page: number;
  readonly name: string;
  readonly kind: string;
  /** `x, y, width, height` in PDF space. */
  readonly rect: Float64Array;
  readonly existing: boolean;
  readonly multiline: boolean;
  readonly maxLength: number | undefined;
  /** A dropdown's choices, or a radio group's export values. */
  readonly options: string[];
  /** Each radio button's `x, y, width, height`, in `options` order. */
  readonly optionRects: Float64Array;
  readonly editable: boolean;
  readonly text: string;
  readonly checked: boolean;
  /** `undefined` (typed `string` by wasm-bindgen) when nothing is chosen. */
  readonly selected: string | null | undefined;
}

/** The largest id the core's `u32` field ids can hold. */
const MAX_FIELD_ID = 2 ** 32 - 1;

/**
 * The core command for `edit`.
 *
 * An id that is not a whole `u32` never reaches the core: wasm-bindgen would
 * wrap it (`1.5` → 1, `-1` → 4294967295) and fill a field nobody named. It is
 * refused here with the core's `FormFieldNotFound` — after the permission,
 * like annotations: when filling is not allowed (`fillable` is false) it is
 * left to the core, which names the permission first.
 */
export function formFieldCommand(edits: CoreFormEdits, edit: FormEdit, fillable: () => boolean): Owned {
  const { fieldId, value } = edit;
  const wellFormed = Number.isInteger(fieldId) && fieldId >= 0 && fieldId <= MAX_FIELD_ID;
  if (!wellFormed && fillable()) throw new EngineError(FORM_FIELD_NOT_FOUND, `no form field with id ${fieldId}`);
  switch (value.kind) {
    case 'text':
      return edits.setFieldText(fieldId, value.text);
    case 'checked':
      return edits.setFieldChecked(fieldId, value.checked);
    case 'choice':
      return edits.setFieldChoice(fieldId, value.option ?? undefined);
  }
}

/**
 * Copies the core's entries into plain fields on the page as drawn and frees
 * them. An entry of a kind this package does not know, on a page past the
 * end, or missing what its kind needs, is left out.
 */
export function formFields(entries: CoreFormField[], frameOf: (page: number) => PageFrame | undefined): FormField[] {
  const decoded: FormField[] = [];
  for (const entry of entries) {
    try {
      const frame = frameOf(entry.page);
      const field = frame && decodeField(entry, frame);
      if (field) decoded.push(field);
    } finally {
      entry.free();
    }
  }
  return decoded;
}

function decodeField(entry: CoreFormField, frame: PageFrame): FormField | null {
  if (entry.rect.length !== 4) return null;
  const base = {
    id: entry.id,
    pageIndex: entry.page,
    name: entry.name,
    rect: frame.rectFromPdf(entry.rect),
    existing: entry.existing,
  };
  const selected = entry.selected ?? null;
  switch (entry.kind) {
    case 'text':
      return { ...base, kind: 'text', multiline: entry.multiline, maxLength: entry.maxLength ?? null, value: entry.text };
    case 'checkbox':
      return { ...base, kind: 'checkbox', checked: entry.checked };
    case 'radio': {
      const buttons = radioButtons(entry, frame);
      return buttons && { ...base, kind: 'radio', buttons, selected };
    }
    case 'dropdown':
      return { ...base, kind: 'dropdown', options: [...entry.options], editable: entry.editable, selected };
    default:
      return null;
  }
}

/** Each button with its own box, or `null` when the values and boxes do not pair up. */
function radioButtons(entry: CoreFormField, frame: PageFrame): RadioButton[] | null {
  const { options, optionRects } = entry;
  if (optionRects.length !== options.length * 4) return null;
  return options.map((exportValue, i) => ({
    exportValue,
    rect: frame.rectFromPdf(optionRects.subarray(i * 4, i * 4 + 4)),
  }));
}
