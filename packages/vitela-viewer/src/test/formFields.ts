// Test support: one form field of each kind, on the pages of a 612 x 792 pt document.

import type { FormField } from '@vitela/core';

type Of<K extends FormField['kind']> = Extract<FormField, { kind: K }>;

const base = { existing: true } as const;

export const NAME: Of<'text'> = {
  ...base,
  id: 1,
  pageIndex: 0,
  kind: 'text',
  name: 'full_name',
  rect: { left: 72, top: 92, width: 200, height: 20 },
  multiline: false,
  maxLength: 100,
  value: 'Ada Lovelace',
};

export const NOTES: Of<'text'> = {
  ...base,
  id: 2,
  pageIndex: 0,
  kind: 'text',
  name: 'notes',
  rect: { left: 72, top: 152, width: 300, height: 60 },
  multiline: true,
  maxLength: null,
  value: '',
};

export const SUBSCRIBE: Of<'checkbox'> = {
  ...base,
  id: 3,
  pageIndex: 0,
  kind: 'checkbox',
  name: 'subscribe',
  rect: { left: 72, top: 240, width: 14, height: 14 },
  checked: true,
};

export const PLAN: Of<'radio'> = {
  ...base,
  id: 4,
  pageIndex: 0,
  kind: 'radio',
  name: 'plan',
  rect: { left: 72, top: 280, width: 74, height: 14 },
  buttons: [
    { exportValue: 'basic', rect: { left: 72, top: 280, width: 14, height: 14 } },
    { exportValue: 'pro', rect: { left: 132, top: 280, width: 14, height: 14 } },
  ],
  selected: 'pro',
};

export const COUNTRY: Of<'dropdown'> = {
  ...base,
  id: 5,
  pageIndex: 1,
  kind: 'dropdown',
  name: 'country',
  rect: { left: 72, top: 330, width: 120, height: 20 },
  options: ['Argentina', 'Uruguay', 'Chile'],
  editable: false,
  selected: 'Uruguay',
};

export const ALL_FIELDS: readonly FormField[] = [NAME, NOTES, SUBSCRIBE, PLAN, COUNTRY];
