// Extract and split across the worker boundary (T-254): the core's split
// plan (core/pdf-wasm/src/engine/extract.rs) becomes the plain `SplitPart`s
// of core/types.ts, and the wasm objects it came in are freed.

import type { SplitPart } from '../core/types.js';
import type { Owned } from './pageText.js';

/** One part as core/pdf-wasm hands it over: a wasm object. */
export interface CoreSplitPart extends Owned {
  readonly first: number;
  readonly last: number;
  readonly fileName: string;
}

/** core/pdf-wasm's free functions for a split. */
export interface CoreSplitFunctions {
  /** Throws `InvalidPageSelection` with the core's sentence. */
  planSplit(cuts: string, totalPages: number, documentName: string): CoreSplitPart[];
}

/** The parts as plain values, each wasm object freed once read. */
export function splitParts(parts: readonly CoreSplitPart[]): SplitPart[] {
  return parts.map((part) => {
    try {
      return { first: part.first, last: part.last, fileName: part.fileName };
    } finally {
      part.free();
    }
  });
}
