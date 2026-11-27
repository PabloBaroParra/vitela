// Document properties across the worker boundary: the `MetadataEdit` becomes
// core/pdf-wasm's `setDocumentProperties` `EditCommand`, and the core's
// `DocumentPropertiesEntry` (a wasm object) becomes plain `DocumentProperties`.
//
// Every rule is the core's (core/pdf-wasm/src/model/metadata.rs): the gate,
// the empty key removed, the dates kept, the unchanged value recording
// nothing. This only renames `null` and `undefined` across the line:
// wasm-bindgen's `None` arrives as `undefined` and is typed as present in its
// `.d.ts` (T-227), so every key is read as `?? null`.

import type { DocumentProperties, MetadataEdit } from '../core/types.js';
import type { Owned } from './pageText.js';

/** core/pdf-wasm's properties `EditCommand` constructor; `undefined` is an absent key. */
export interface CoreMetadataEdits {
  setDocumentProperties(
    title: string | undefined,
    author: string | undefined,
    subject: string | undefined,
    keywords: string | undefined,
    creator: string | undefined,
    producer: string | undefined,
  ): Owned;
}

/** core/pdf-wasm's `DocumentPropertiesEntry`. */
export interface CoreDocumentProperties extends Owned {
  readonly title: string | null | undefined;
  readonly author: string | null | undefined;
  readonly subject: string | null | undefined;
  readonly keywords: string | null | undefined;
  readonly creator: string | null | undefined;
  readonly producer: string | null | undefined;
}

/** The core command for `edit`, its keys in the core's order. */
export function metadataCommand(edits: CoreMetadataEdits, { properties: p }: MetadataEdit): Owned {
  const key = (value: string | null) => value ?? undefined;
  return edits.setDocumentProperties(
    key(p.title),
    key(p.author),
    key(p.subject),
    key(p.keywords),
    key(p.creator),
    key(p.producer),
  );
}

/** Copies the core's entry into plain properties and frees it. */
export function documentProperties(entry: CoreDocumentProperties): DocumentProperties {
  try {
    return {
      title: entry.title ?? null,
      author: entry.author ?? null,
      subject: entry.subject ?? null,
      keywords: entry.keywords ?? null,
      creator: entry.creator ?? null,
      producer: entry.producer ?? null,
    };
  } finally {
    entry.free();
  }
}
