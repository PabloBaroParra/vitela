// Export images, as text: the dialog's choices before they are checked, the
// resolution box, and every sentence the feature shows. Pure. The words are
// the native shells' (Android's strings, which are Windows' and GTK's).

import type { ImageExportFormat } from '@vitela/core';
import { sentence } from '../../ui/text';

/** Which pages an export covers. */
export type ExportPages = 'all' | 'current' | 'custom';

/**
 * What the dialog shows. `dpi` and `customRange` stay text, as typed;
 * `planImageExport` is what reads them.
 */
export interface ExportDraft {
  readonly pages: ExportPages;
  readonly customRange: string;
  readonly dpi: string;
  readonly format: ImageExportFormat;
}

/**
 * The ceiling is 400, not the 600 an archival scan would suggest: the core
 * refuses any raster over 32 Mpx, and US Letter at 600 DPI is 33.7 Mpx. The
 * Windows, GTK and Android shells stop at the same place for the same reason.
 */
export const MIN_EXPORT_DPI = 72;
export const MAX_EXPORT_DPI = 400;
export const DEFAULT_EXPORT_DPI = 150;

export const DEFAULT_DRAFT: ExportDraft = { pages: 'all', customRange: '', dpi: String(DEFAULT_EXPORT_DPI), format: 'png' };

export const EXPORT_FORBIDDEN = 'This document does not permit extracting its pages as images.';
export const NO_PAGES = 'This document has no pages to export.';
export const NO_FORMATS = 'This PDF engine offers no image format to export to.';

/**
 * The resolution typed: a cleared box reads as the default, like the native
 * boxes; anything that is not a whole number in range is `null`, never
 * rounded into one.
 */
export function readDpi(typed: string): number | null {
  const text = typed.trim();
  if (text === '') return DEFAULT_EXPORT_DPI;
  if (!/^\d+$/.test(text)) return null;
  const dpi = Number(text);
  return dpi >= MIN_EXPORT_DPI && dpi <= MAX_EXPORT_DPI ? dpi : null;
}

export function formatLabel(format: ImageExportFormat): string {
  return format === 'png' ? 'PNG' : 'JPEG';
}

/** The media type a downloaded image is labelled with. */
export function imageMediaType(format: ImageExportFormat): string {
  return format === 'png' ? 'image/png' : 'image/jpeg';
}

export function dpiRangeRefusal(): string {
  return `Choose a resolution between ${MIN_EXPORT_DPI} and ${MAX_EXPORT_DPI} DPI.`;
}

/** `page` is one-based. */
export function pageTooLarge(page: number, dpi: number): string {
  return `Page ${page} is too large to export at ${dpi} DPI. Choose a lower resolution.`;
}

/** `page` is one-based; `index` counts the files of this export from 1. */
export function exportProgress(page: number, index: number, total: number): string {
  return `Exporting page ${page} (${index} of ${total})…`;
}

export function exportDone(count: number, format: ImageExportFormat): string {
  return `Exported ${count} ${count === 1 ? 'page' : 'pages'} as ${formatLabel(format)}.`;
}

/** `page` is one-based; `written` is how many files went out before it. */
export function pageFailed(page: number, message: string, written: number): string {
  return `Page ${page} could not be exported: ${sentence(message)} ${written} written.`;
}

export function writeFailed(fileName: string, written: number): string {
  return `Could not write ${fileName}. ${written} written.`;
}

export function exportCancelled(written: number): string {
  return written === 0 ? 'Export cancelled. No file was written.' : `Export cancelled. ${written} written.`;
}

export function openFailed(message: string): string {
  return `Could not export: ${sentence(message)} No file was written.`;
}
