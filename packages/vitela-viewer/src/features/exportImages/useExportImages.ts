// Export images: choices, then a plan, then one file per page — the order of
// the native shells. Every choice is checked while the dialog is still open
// (planExport.ts), so a bad range or an oversized page is fixed where it was
// typed instead of being found after some files are already out.
//
// The pages are drawn from the engine's export snapshot (what a save would
// write now), so unsaved page edits, annotations and fills are in the
// images, as on Android and Windows. Each page goes out as its own file the
// moment it is encoded, through the same delivery as a compressed copy (a
// download, or the host's onSave), and the export stops at the first page
// that fails: a missing file among many goes unnoticed, so what was written
// is counted instead. Nothing about the open document changes.
// Container logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage, type Engine } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import type { Delivery } from '../save/delivery';
import {
  DEFAULT_DRAFT,
  EXPORT_FORBIDDEN,
  exportDone,
  imageMediaType,
  openFailed,
  pageFailed,
  writeFailed,
  type ExportDraft,
} from './exportText';
import { planImageExport, type ExportPlan } from './planExport';

export type ExportActivity =
  | { readonly kind: 'idle' }
  /** The dialog is open. */
  | {
      readonly kind: 'choosing';
      readonly draft: ExportDraft;
      /** Whether the document lets its pages leave as images at all. */
      readonly allowed: boolean;
      /** Why the last Export was refused, or why none is possible; `null` when nothing was. */
      readonly message: string | null;
      /** The choices are being checked: Export waits. */
      readonly checking: boolean;
    }
  /** `page` is one-based; `index` counts this export's files from 1. */
  | { readonly kind: 'exporting'; readonly page: number; readonly index: number; readonly total: number }
  | { readonly kind: 'done'; readonly message: string }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: ExportActivity = { kind: 'idle' };

/** From the click until the dialog closes or the export ends: Export images cannot be started again. */
export function exportBusy(activity: ExportActivity): boolean {
  return activity.kind === 'choosing' || activity.kind === 'exporting';
}

export type ExportImagesSession = ReturnType<typeof useExportImages>;

export function useExportImages(
  engine: Engine,
  document: OpenDocument | null,
  delivery: Delivery,
  /** Settles once the edits the editing screen has asked for so far have ended. */
  settleEdits?: () => Promise<unknown>,
  /** The page the reader is on, zero-based. */
  currentPage: () => number = () => 0,
) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<ExportActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document on screen: an answer for an old document changes nothing. */
  const shown = useRef(generation);
  /** The latest document: its page count after the queued edits settled. */
  const latest = useRef(document);
  /** True from the click until the dialog closes or the export ends: one at a time. */
  const busy = useRef(false);

  // A new document starts clean (adjusting state while rendering).
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
  }
  useEffect(() => {
    latest.current = document;
  }, [document]);
  useEffect(() => {
    shown.current = generation;
    busy.current = false;
  }, [generation]);

  const open = useCallback(() => {
    if (!document || busy.current) return;
    busy.current = true;
    const allowed = document.info.textExtractionAllowed;
    setActivity({ kind: 'choosing', draft: DEFAULT_DRAFT, allowed, message: allowed ? null : EXPORT_FORBIDDEN, checking: false });
  }, [document]);

  /** A changed choice clears the last refusal: it was about the old one. */
  const edit = useCallback((draft: ExportDraft) => {
    setActivity((a) => (a.kind === 'choosing' && a.allowed && !a.checking ? { ...a, draft, message: null } : a));
  }, []);

  const cancel = useCallback(() => {
    if (activity.kind !== 'choosing' || activity.checking) return;
    setActivity(IDLE);
    busy.current = false;
  }, [activity]);

  const run = useCallback(
    async (plan: ExportPlan, handle: OpenDocument['handle'], current: () => boolean) => {
      const { files, dpi, format } = plan;
      let written = 0;
      let exportHandle: number;
      try {
        exportHandle = await engine.openImageExport(handle);
      } catch (error) {
        if (current()) setActivity({ kind: 'failed', message: openFailed(errorMessage(error)) });
        return;
      }
      try {
        for (const [i, { pageIndex, fileName }] of files.entries()) {
          if (!current()) return;
          setActivity({ kind: 'exporting', page: pageIndex + 1, index: i + 1, total: files.length });
          let bytes: Uint8Array;
          try {
            bytes = await engine.exportPageImage(exportHandle, pageIndex, dpi, format);
          } catch (error) {
            if (current()) setActivity({ kind: 'failed', message: pageFailed(pageIndex + 1, errorMessage(error), written) });
            return;
          }
          try {
            await delivery.deliver({ bytes, name: fileName, type: imageMediaType(format), destination: { kind: 'newFile' } });
          } catch {
            if (current()) setActivity({ kind: 'failed', message: writeFailed(fileName, written) });
            return;
          }
          written++;
        }
        if (current()) setActivity({ kind: 'done', message: exportDone(written, format) });
      } finally {
        // The snapshot is the engine's memory: freed however the export ended.
        await engine.closeImageExport(exportHandle).catch(() => false);
      }
    },
    [engine, delivery],
  );

  const exportImages = useCallback(async () => {
    if (!document || activity.kind !== 'choosing' || !activity.allowed || activity.checking) return;
    const { draft } = activity;
    const running = document.generation;
    const current = () => shown.current === running;
    setActivity({ ...activity, checking: true, message: null });
    /** A refusal keeps the dialog open: the export is still the user's to fix. */
    let keepDialog = false;
    try {
      // The edits asked for before the click go in, and the plan counts their pages.
      await settleEdits?.();
      const now = latest.current ?? document;
      const planned = await planImageExport(engine, now, draft, currentPage());
      if (!current()) return;
      if (planned.kind === 'refused') {
        keepDialog = true;
        // Only the message changes: the dialog stays open on what was typed.
        setActivity((a) => (a.kind === 'choosing' ? { ...a, checking: false, message: planned.message } : a));
        return;
      }
      await run(planned.plan, now.handle, current);
    } catch (error) {
      if (current()) setActivity({ kind: 'failed', message: openFailed(errorMessage(error)) });
    } finally {
      if (current() && !keepDialog) busy.current = false;
    }
  }, [engine, document, settleEdits, currentPage, activity, run]);

  return { activity, open, edit, cancel, exportImages };
}
