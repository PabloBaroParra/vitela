// Export images, the plan: every choice in the dialog checked against the
// document, and the files the export would write — or the reason it cannot.
// The order is the native shells': the resolution, the permission, the
// pages, the raster ceiling, then the names. Every answer past the
// resolution is the core's (the page grammar, the ceiling, the names), so
// the web cannot disagree with the other shells about what "7-3" means or
// what page 3 of report.pdf is called. Checked while the dialog is still
// open, so a bad range is fixed where it was typed.
//
// The pages are counted from the export's snapshot (`openImageExport`), not
// from the document as the shell last saw it: an edit still settling when
// Export was pressed has changed the count by the time the snapshot exists.
// What needs no snapshot (the resolution, the permission) is checked first,
// so a choice that cannot work never costs one.

import { errorKind, errorMessage, INVALID_PAGE_SELECTION, type DocumentHandle, type Engine, type ImageExportFormat } from '@vitela/core';
import { sentence } from '../../ui/text';
import { EXPORT_FORBIDDEN, NO_PAGES, dpiRangeRefusal, pageTooLarge, readDpi, type ExportDraft } from './exportText';

/** One file an export writes: the page, and the name the core gave it. */
export interface ExportFile {
  readonly pageIndex: number;
  readonly fileName: string;
}

/** An export every choice of which has been checked: only the renderer or the disk can still refuse it. */
export interface ExportPlan {
  readonly files: readonly ExportFile[];
  readonly dpi: number;
  readonly format: ImageExportFormat;
}

export type PlanResult = { readonly kind: 'plan'; readonly plan: ExportPlan } | { readonly kind: 'refused'; readonly message: string };

type PlanningEngine = Pick<Engine, 'parsePageSelection' | 'firstPageTooLargeToExport' | 'pageImageFileNames'>;

interface PlannedDocument {
  readonly handle: DocumentHandle;
  readonly name: string;
  readonly textExtractionAllowed: boolean;
  /** The export snapshot's page count. */
  readonly pageCount: number;
}

/** Why `draft` cannot be exported before any snapshot is taken, or `null`. */
export function precheckExport(draft: ExportDraft, textExtractionAllowed: boolean): string | null {
  if (readDpi(draft.dpi) === null) return dpiRangeRefusal();
  if (!textExtractionAllowed) return EXPORT_FORBIDDEN;
  return null;
}

/**
 * The files `draft` would write from `document`. `currentPage` (zero-based)
 * is the page the reader is on. A refusal is an answer, for the dialog to
 * show; an engine failure other than an unreadable range rejects.
 */
export async function planImageExport(
  engine: PlanningEngine,
  document: PlannedDocument,
  draft: ExportDraft,
  currentPage: number,
): Promise<PlanResult> {
  const refused = (message: string): PlanResult => ({ kind: 'refused', message });
  const early = precheckExport(draft, document.textExtractionAllowed);
  if (early !== null) return refused(early);
  const dpi = readDpi(draft.dpi)!;
  const total = document.pageCount;
  if (total === 0) return refused(NO_PAGES);

  let pages: readonly number[];
  switch (draft.pages) {
    case 'all':
      pages = Array.from({ length: total }, (_, page) => page);
      break;
    case 'current':
      // Clamped rather than trusted: a removal can leave the reader one past the end until it catches up.
      pages = [Math.min(Math.max(currentPage, 0), total - 1)];
      break;
    case 'custom':
      try {
        pages = await engine.parsePageSelection(draft.customRange, total);
      } catch (error) {
        if (errorKind(error) === INVALID_PAGE_SELECTION) return refused(sentence(errorMessage(error)));
        throw error;
      }
      break;
  }

  const oversized = await engine.firstPageTooLargeToExport(document.handle, pages, dpi);
  if (oversized !== null) return refused(pageTooLarge(oversized + 1, dpi));

  const names = await engine.pageImageFileNames(document.name, pages, total, draft.format);
  const files = pages.map((pageIndex, i) => ({ pageIndex, fileName: names[i]! }));
  return { kind: 'plan', plan: { files, dpi, format: draft.format } };
}
