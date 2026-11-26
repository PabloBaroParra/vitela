import { describe, expect, it } from 'vitest';
import { COUNTRY, NAME, NOTES, PLAN, SUBSCRIBE } from '../../test/formFields';
import { fieldsByPage, paintKeys, sameValue } from './fieldValues';

describe('sameValue', () => {
  it('compares a text field with the text it holds', () => {
    expect(sameValue(NAME, { kind: 'text', text: 'Ada Lovelace' })).toBe(true);
    expect(sameValue(NAME, { kind: 'text', text: 'Ada' })).toBe(false);
    expect(sameValue(NOTES, { kind: 'text', text: '' })).toBe(true);
  });

  it('compares a checkbox with whether it is checked', () => {
    expect(sameValue(SUBSCRIBE, { kind: 'checked', checked: true })).toBe(true);
    expect(sameValue(SUBSCRIBE, { kind: 'checked', checked: false })).toBe(false);
  });

  it('compares a radio group or a dropdown with its choice, none included', () => {
    expect(sameValue(PLAN, { kind: 'choice', option: 'pro' })).toBe(true);
    expect(sameValue(PLAN, { kind: 'choice', option: 'basic' })).toBe(false);
    expect(sameValue(COUNTRY, { kind: 'choice', option: 'Uruguay' })).toBe(true);
    expect(sameValue(COUNTRY, { kind: 'choice', option: null })).toBe(false);
    expect(sameValue({ ...COUNTRY, selected: null }, { kind: 'choice', option: null })).toBe(true);
  });

  it('never calls a value of the wrong kind the same: the core decides about it', () => {
    expect(sameValue(NAME, { kind: 'checked', checked: true })).toBe(false);
    expect(sameValue(SUBSCRIBE, { kind: 'choice', option: null })).toBe(false);
    expect(sameValue(PLAN, { kind: 'text', text: 'pro' })).toBe(false);
  });
});

describe('fieldsByPage', () => {
  it('groups the fields by page, in the order they come', () => {
    expect(fieldsByPage([NAME, COUNTRY, NOTES])).toEqual([
      [0, [NAME, NOTES]],
      [1, [COUNTRY]],
    ]);
    expect(fieldsByPage([])).toEqual([]);
  });
});

describe('paintKeys', () => {
  it('gives each page with fields a key that changes with what they hold', () => {
    const before = paintKeys([NAME, SUBSCRIBE, COUNTRY]);
    const after = paintKeys([{ ...NAME, value: 'Grace Hopper' }, SUBSCRIBE, COUNTRY]);
    expect([...before.keys()]).toEqual([0, 1]);
    expect(after.get(0)).not.toBe(before.get(0));
    expect(after.get(1)).toBe(before.get(1));
  });

  it('keeps the key when nothing it shows changed', () => {
    expect(paintKeys([NAME, PLAN])).toEqual(paintKeys([{ ...NAME }, { ...PLAN }]));
    expect(paintKeys([PLAN]).get(0)).not.toBe(paintKeys([{ ...PLAN, selected: 'basic' }]).get(0));
    expect(paintKeys([SUBSCRIBE]).get(0)).not.toBe(paintKeys([{ ...SUBSCRIBE, checked: false }]).get(0));
  });

  it('gives no key to a page whose fields hold what they held first: its bitmap already shows them', () => {
    const first = paintKeys([NAME, COUNTRY]);
    expect(paintKeys([NAME, COUNTRY], first)).toEqual(
      new Map([
        [0, ''],
        [1, ''],
      ]),
    );
    const filled = paintKeys([{ ...NAME, value: 'Grace' }, COUNTRY], first);
    expect(filled.get(0)).not.toBe('');
    expect(filled.get(1)).toBe('');
    // Undone back to what it held: a key again unlike the filled one, so it is painted again.
    expect(paintKeys([NAME, COUNTRY], first).get(0)).not.toBe(filled.get(0));
  });
});
