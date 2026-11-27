// Image export across the worker boundary (T-252): the core's format names,
// its page grammar and its file names (core/pdf-wasm/src/engine/export.rs)
// become the plain values in core/types.ts.
//
// A format this package does not know is never offered: it is left out of
// the list, as compress.ts leaves out a preset it does not know.

import type { ImageExportFormat } from '../core/types.js';

/** core/pdf-wasm's free functions for an export. */
export interface CoreExportFunctions {
  /** The core's format names, in its order. */
  imageExportFormats(): string[];
  /** Zero-based positions; throws `InvalidPageSelection`. */
  parsePageSelection(input: string, totalPages: number): Uint32Array;
  pageImageFileName(documentName: string, pageIndex: number, totalPages: number, format: string): string;
}

const FORMATS: readonly ImageExportFormat[] = ['png', 'jpeg'];

/** The core's formats this package can offer, in the core's order. */
export function imageExportFormats(names: readonly string[]): ImageExportFormat[] {
  return names.filter((name): name is ImageExportFormat => (FORMATS as readonly string[]).includes(name));
}
