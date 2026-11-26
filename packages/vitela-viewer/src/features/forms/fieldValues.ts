// What a form field holds, compared and grouped. Pure.

import type { FieldValue, FormField } from '@vitela/core';

/**
 * Whether filling `field` with `value` would leave it as it is. The core
 * records such a fill as a step all the same, so it is never sent. A value of
 * the wrong kind is never the same: the core is the one to refuse it.
 */
export function sameValue(field: FormField, value: FieldValue): boolean {
  switch (field.kind) {
    case 'text':
      return value.kind === 'text' && value.text === field.value;
    case 'checkbox':
      return value.kind === 'checked' && value.checked === field.checked;
    case 'radio':
    case 'dropdown':
      return value.kind === 'choice' && value.option === field.selected;
  }
}

/** Whether two values asked for a field are the same. */
export function sameFieldValue(a: FieldValue, b: FieldValue): boolean {
  if (a.kind === 'text' && b.kind === 'text') return a.text === b.text;
  if (a.kind === 'checked' && b.kind === 'checked') return a.checked === b.checked;
  if (a.kind === 'choice' && b.kind === 'choice') return a.option === b.option;
  return false;
}

/** The fields of each page, in the order they come (the core's: `/Fields`). */
export function fieldsByPage(fields: readonly FormField[]): [number, FormField[]][] {
  const pages = new Map<number, FormField[]>();
  for (const field of fields) {
    const list = pages.get(field.pageIndex);
    if (list) list.push(field);
    else pages.set(field.pageIndex, [field]);
  }
  return [...pages];
}

/** What a field shows on its page. */
function shown(field: FormField): string | boolean | null {
  switch (field.kind) {
    case 'text':
      return field.value;
    case 'checkbox':
      return field.checked;
    case 'radio':
    case 'dropdown':
      return field.selected;
  }
}

/**
 * For each page with fields, a key that changes whenever what its fields
 * hold does. pdfium paints the fields into the page's bitmap, so a fill, or
 * the undo of one, has to paint the page again; nothing else about the page
 * says it changed. A page whose key is the one it had in `baseline` (the
 * fields as first listed, which its bitmap was painted with) gets `''`, so
 * listing the fields paints nothing again.
 */
export function paintKeys(fields: readonly FormField[], baseline?: ReadonlyMap<number, string>): Map<number, string> {
  return new Map(
    fieldsByPage(fields).map(([pageIndex, onPage]) => {
      const key = JSON.stringify(onPage.map((field) => [field.id, shown(field)]));
      return [pageIndex, key === baseline?.get(pageIndex) ? '' : key];
    }),
  );
}
