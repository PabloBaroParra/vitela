// The words and the draft of the Document properties dialog: the six text
// keys of `/Info` with Android's labels and order, and what an apply should
// send. Pure.

import type { DocumentProperties } from '@vitela/core';

export type PropertyKey = keyof DocumentProperties;

/** What the dialog's boxes hold: one string per key, empty for an absent one. */
export type PropertiesDraft = { readonly [Key in PropertyKey]: string };

/** The dialog's boxes, in order. */
export const PROPERTY_FIELDS: readonly { readonly key: PropertyKey; readonly label: string }[] = [
  { key: 'title', label: 'Title' },
  { key: 'author', label: 'Author' },
  { key: 'subject', label: 'Subject' },
  { key: 'keywords', label: 'Keywords' },
  { key: 'creator', label: 'Creator' },
  { key: 'producer', label: 'Producer' },
];

/** The status once a change is recorded (Android's words). */
export const PROPERTIES_UPDATED = 'Document properties updated. Changes are pending save.';

export function draftOf(properties: DocumentProperties): PropertiesDraft {
  return {
    title: properties.title ?? '',
    author: properties.author ?? '',
    subject: properties.subject ?? '',
    keywords: properties.keywords ?? '',
    creator: properties.creator ?? '',
    producer: properties.producer ?? '',
  };
}

/**
 * What an apply should record, or `null` when `draft` says nothing new. A
 * text box cannot tell "cleared" from "never set", so an empty one is an
 * absent key, never `""` — Android's `metadataChange` (the core reads `""`
 * the same way).
 */
export function propertiesChange(current: DocumentProperties, draft: PropertiesDraft): DocumentProperties | null {
  const key = (value: string) => (value === '' ? null : value);
  const after: DocumentProperties = {
    title: key(draft.title),
    author: key(draft.author),
    subject: key(draft.subject),
    keywords: key(draft.keywords),
    creator: key(draft.creator),
    producer: key(draft.producer),
  };
  return PROPERTY_FIELDS.some(({ key: name }) => after[name] !== current[name]) ? after : null;
}
