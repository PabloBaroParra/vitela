import { describe, expect, it } from 'vitest';
import { PageFrame } from './annotations.js';
import { FORM_FIELD_NOT_FOUND } from '../core/errors.js';
import { formFieldCommand, formFields, type CoreFormEdits, type CoreFormField } from './forms.js';
import type { CoreGeometry, Owned } from './pageText.js';

/** An upright page: PDF y grows up from the bottom, drawn y down from the top. */
const flipping: CoreGeometry = {
  placeRects: (rects, _width, height) => {
    const placed = new Float32Array(rects.length);
    for (let i = 0; i + 3 < rects.length; i += 4) {
      placed.set([rects[i]!, height - rects[i + 1]! - rects[i + 3]!, rects[i + 2]!, rects[i + 3]!], i);
    }
    return placed;
  },
  pointToPdf: (x, y, _width, height) => new Float32Array([x, height - y]),
};
const page = new PageFrame({ widthPt: 612, heightPt: 792, rotationDegrees: 0 }, flipping);
const frameOf = (index: number) => (index === 0 ? page : undefined);

function entry(fields: Partial<CoreFormField>, freed: string[] = []): CoreFormField {
  return {
    id: 0,
    page: 0,
    name: 'full_name',
    kind: 'text',
    rect: new Float64Array([72, 680, 240, 22]),
    existing: true,
    multiline: false,
    maxLength: undefined,
    options: [],
    optionRects: new Float64Array(0),
    editable: false,
    text: '',
    checked: false,
    selected: undefined,
    free: () => freed.push(`field:${fields.id ?? 0}`),
    ...fields,
  };
}

describe('formFields', () => {
  it('decodes each kind on the page as drawn, and frees every entry', () => {
    const freed: string[] = [];
    const decoded = formFields(
      [
        entry({ id: 0, text: 'José Peña', maxLength: 100 }, freed),
        entry({ id: 1, name: 'notes', multiline: true }, freed),
        entry({ id: 2, name: 'subscribe', kind: 'checkbox', checked: true, existing: false }, freed),
        entry(
          {
            id: 3,
            name: 'plan',
            kind: 'radio',
            rect: new Float64Array([72, 500, 78, 18]),
            options: ['basic', 'pro'],
            optionRects: new Float64Array([72, 500, 18, 18, 132, 500, 18, 18]),
            selected: 'pro',
          },
          freed,
        ),
        entry({ id: 4, name: 'country', kind: 'dropdown', options: ['Argentina', 'Uruguay'], selected: undefined }, freed),
      ],
      frameOf,
    );

    const rect = { left: 72, top: 90, width: 240, height: 22 };
    expect(decoded).toEqual([
      { id: 0, pageIndex: 0, name: 'full_name', rect, existing: true, kind: 'text', multiline: false, maxLength: 100, value: 'José Peña' },
      { id: 1, pageIndex: 0, name: 'notes', rect, existing: true, kind: 'text', multiline: true, maxLength: null, value: '' },
      { id: 2, pageIndex: 0, name: 'subscribe', rect, existing: false, kind: 'checkbox', checked: true },
      {
        id: 3,
        pageIndex: 0,
        name: 'plan',
        rect: { left: 72, top: 274, width: 78, height: 18 },
        existing: true,
        kind: 'radio',
        buttons: [
          { exportValue: 'basic', rect: { left: 72, top: 274, width: 18, height: 18 } },
          { exportValue: 'pro', rect: { left: 132, top: 274, width: 18, height: 18 } },
        ],
        selected: 'pro',
      },
      {
        id: 4,
        pageIndex: 0,
        name: 'country',
        rect,
        existing: true,
        kind: 'dropdown',
        options: ['Argentina', 'Uruguay'],
        editable: false,
        selected: null,
      },
    ]);
    expect(freed).toEqual(['field:0', 'field:1', 'field:2', 'field:3', 'field:4']);
  });

  it('leaves out an unknown kind, a broken rect, a radio whose buttons do not match, and a page past the end', () => {
    const freed: string[] = [];
    const decoded = formFields(
      [
        entry({ id: 0, kind: 'signature' }, freed),
        entry({ id: 1, rect: new Float64Array([1, 2]) }, freed),
        entry({ id: 2, kind: 'radio', options: ['a', 'b'], optionRects: new Float64Array([0, 0, 1, 1]) }, freed),
        entry({ id: 3, page: 4 }, freed),
        entry({ id: 4 }, freed),
      ],
      frameOf,
    );
    expect(decoded.map((field) => field.id)).toEqual([4]);
    expect(freed).toHaveLength(5);
  });
});

describe('formFieldCommand', () => {
  const sent: unknown[][] = [];
  const owned = (...args: unknown[]): Owned => (sent.push(args), { free: () => {} });
  const edits: CoreFormEdits = {
    setFieldText: (id, text) => owned('text', id, text),
    setFieldChecked: (id, checked) => owned('checked', id, checked),
    setFieldChoice: (id, option) => owned('choice', id, option),
  };

  it('sends each kind of value to its own constructor, nothing chosen as undefined', () => {
    sent.length = 0;
    formFieldCommand(edits, { kind: 'setFieldValue', fieldId: 3, value: { kind: 'text', text: 'Ada' } }, () => true);
    formFieldCommand(edits, { kind: 'setFieldValue', fieldId: 4, value: { kind: 'checked', checked: false } }, () => true);
    formFieldCommand(edits, { kind: 'setFieldValue', fieldId: 5, value: { kind: 'choice', option: 'pro' } }, () => true);
    formFieldCommand(edits, { kind: 'setFieldValue', fieldId: 5, value: { kind: 'choice', option: null } }, () => true);
    expect(sent).toEqual([
      ['text', 3, 'Ada'],
      ['checked', 4, false],
      ['choice', 5, 'pro'],
      ['choice', 5, undefined],
    ]);
  });

  it.each([-1, 1.5, Number.NaN, 2 ** 32])(
    'refuses id %s as FormFieldNotFound before the core, which would wrap it onto a real id',
    (fieldId) => {
      sent.length = 0;
      const fill = () =>
        formFieldCommand(edits, { kind: 'setFieldValue', fieldId, value: { kind: 'text', text: 'Ada' } }, () => true);
      expect(fill).toThrow(expect.objectContaining({ name: FORM_FIELD_NOT_FOUND }));
      expect(sent).toEqual([]);
    },
  );

  it('leaves a malformed id to the core when filling is not permitted, so the permission is named first', () => {
    sent.length = 0;
    formFieldCommand(edits, { kind: 'setFieldValue', fieldId: -1, value: { kind: 'checked', checked: true } }, () => false);
    expect(sent).toHaveLength(1);
  });
});
