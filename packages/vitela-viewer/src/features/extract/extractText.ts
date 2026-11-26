// Extract pages and Split, as text: the names, the dialog hints and every
// status sentence. Pure. The dialog and status words are the native shells'
// (Android's strings, which are the Windows shell's); the file names a
// split writes are the core's (`planSplit`), and an extract's is the one
// Android and Windows suggest.

import type { SaveOutcome } from '../save/outcome';
import { sentence } from '../../ui/text';

/** A run of zero-based pages. */
export interface PageRun {
  readonly from: number;
  readonly count: number;
}

export const EXTRACT_RANGE_MISSING = 'Type which pages to extract, for example 1-3,7.';

const EXTRACT_SIGNED = "The original document is signed, so the extracted PDF's signature no longer verifies.";
const SPLIT_SIGNED = 'The original document is signed, so the signature in each part no longer verifies.';

const pages = (count: number) => `${count} ${count === 1 ? 'page' : 'pages'}`;

/** The name the extracted PDF is delivered under: "report.pdf" -> "report-extract.pdf". */
export function extractedName(name: string): string {
  const stem = name.trim().replace(/\.pdf$/i, '').trim();
  return `${stem || 'document'}-extract.pdf`;
}

/** A run of pages in the grammar the dialog reads (`"2-3"`): what an Organize selection fills it with. */
export function pageRunText({ from, count }: PageRun): string {
  return count === 1 ? String(from + 1) : `${from + 1}-${from + count}`;
}

export function extractRangeHint(pageCount: number): string {
  return `This document has ${pages(pageCount)}.`;
}

export function splitHint(pageCount: number): string {
  return `This document has ${pages(pageCount)}. Each cut starts a new file.`;
}

/** The status line for a finished extraction of `count` pages. */
export function extractStatus(outcome: SaveOutcome, count: number, signed: boolean): string {
  if (outcome.how === 'cancelled') return 'Extract cancelled. No file was written.';
  const verb = outcome.how === 'delivered' || outcome.how === 'wrote' ? 'Saved' : 'Downloaded';
  const done = `${verb} ${outcome.name}: ${pages(count)} extracted to a new PDF.`;
  return signed ? `${done} ${EXTRACT_SIGNED}` : done;
}

/** `index` counts the parts from 1. */
export function splitProgress(index: number, total: number): string {
  return `Writing part ${index} of ${total}…`;
}

export function splitDone(count: number, signed: boolean): string {
  const done = `Split into ${count} PDFs.`;
  return signed ? `${done} ${SPLIT_SIGNED}` : done;
}

export function splitCancelled(written: number): string {
  return written === 0 ? 'Split cancelled. No file was written.' : `Split cancelled. ${written} written.`;
}

/** `written` is how many parts went out before this one. */
export function splitPartFailed(fileName: string, message: string, written: number): string {
  return `Split stopped: ${fileName} could not be made: ${sentence(message)} ${written} written.`;
}

export function splitWriteFailed(fileName: string, written: number): string {
  return `Split stopped: could not write ${fileName}. ${written} written.`;
}
