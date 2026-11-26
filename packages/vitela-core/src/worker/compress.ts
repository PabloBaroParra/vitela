// Compression across the worker boundary (T-250): the core's preset names,
// its refusal and its compressed save (core/pdf-wasm/src/dto/compress.rs)
// become the plain values in core/types.ts, and every wasm object is freed.
//
// A name this package does not know is never guessed at: a preset it cannot
// offer is left out of the list, and a refusal of a kind it does not model
// reads as `other`, keeping the core's message so the reader still learns why.

import type { CompressedDocument, CompressPreset, CompressRefusal, CompressRefusalKind } from '../core/types.js';
import type { Owned } from './pageText.js';

/** core/pdf-wasm's `CompressRefusalEntry`. */
export interface CoreCompressRefusal extends Owned {
  readonly kind: string;
  readonly message: string;
}

/** core/pdf-wasm's `CompressedSave`. */
export interface CoreCompressedSave extends Owned {
  readonly beforeBytes: number;
  readonly afterBytes: number;
  readonly savedBytes: number;
  readonly reduced: boolean;
  /** Fresh wasm objects on every read: each is freed. */
  readonly refusals: CoreCompressRefusal[];
  takeBytes(): Uint8Array;
}

const PRESETS: readonly CompressPreset[] = ['lossless', 'balanced', 'small'];
const REFUSALS: readonly CompressRefusalKind[] = ['encryptedDocumentNotRewritable', 'signaturesWouldBeInvalidated'];

/** The core's presets this package can offer, in the core's order. */
export function compressPresets(names: readonly string[]): CompressPreset[] {
  return names.filter((name): name is CompressPreset => (PRESETS as readonly string[]).includes(name));
}

/** The core's refusal as a plain value (freeing it), or `null` for none (wasm-bindgen's `undefined`). */
export function compressRefusal(entry: CoreCompressRefusal | null | undefined): CompressRefusal | null {
  if (!entry) return null;
  try {
    const kind = (REFUSALS as readonly string[]).includes(entry.kind) ? (entry.kind as CompressRefusalKind) : 'other';
    return { kind, message: entry.message };
  } finally {
    entry.free();
  }
}

/** The core's compressed save as a plain value, freeing it and its refusals. */
export function compressedDocument(saved: CoreCompressedSave): CompressedDocument {
  try {
    const refusals = saved.refusals.map((entry) => compressRefusal(entry)!);
    return {
      bytes: saved.takeBytes(),
      beforeBytes: saved.beforeBytes,
      afterBytes: saved.afterBytes,
      savedBytes: saved.savedBytes,
      reduced: saved.reduced,
      refusals,
    };
  } finally {
    saved.free();
  }
}
