// Export images: choices, then a plan, then one file per page — the order of
// the native shells. Every choice is checked while the dialog is still open
// (planExport.ts), so a bad range or an oversized page is fixed where it was
// typed instead of being found after some files are already out.
//
// The pages are drawn from the engine's export snapshot (what a save would
// write now), so unsaved page edits, annotations and fills are in the
// images, as on Android and Windows. The snapshot is opened once the queued
// edits have settled and BEFORE the plan, which counts its pages: the
// document as this hook last saw it may not have caught up with an edit yet.
// Each page goes out as its own file the moment it is encoded, through the
// same delivery as a compressed copy (a download, or the host's onSave). The
// export stops at the first page that fails, when the user cancels it (after
// the page in hand), or when the editor goes away; what was written is
// counted, and the snapshot is closed however it ends. The formats offered
// are the core's (`imageExportFormats`). Nothing about the open document
// changes. Container logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage, type Engine, type ImageExportFormat } from '@vitela/core';
import { sentence } from '../../ui/text';
import type { OpenDocument } from '../document/useDocument';
import type { Delivery } from '../save/delivery';
import {
  DEFAULT_DRAFT,
  EXPORT_FORBIDDEN,
  NO_FORMATS,
  exportCancelled,
  exportDone,
  imageMediaType,
  openFailed,
  pageFailed,
  writeFailed,
  type ExportDraft,
} from './exportText';
import { planImageExport, precheckExport, type ExportPlan } from './planExport';

export type ExportActivity =
  | { readonly kind: 'idle' }
  /** The dialog is open. */
  | {
      readonly kind: 'choosing';
      readonly draft: ExportDraft;
      /** The formats the core offers, in its order. */
      readonly formats: readonly ImageExportFormat[];
      /** Whether an export can run at all (the permission, and formats to offer). */
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
  /** False once the editor is gone: a running export stops at its next step. */
  const mounted = useRef(true);
  /** Set by Cancel export: the running export stops after the page in hand. */
  const stopRequested = useRef(false);
  /** True from the click until the dialog closes or the export ends: one at a time. */
  const busy = useRef(false);

  // A new document starts clean (adjusting state while rendering).
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
  }
  useEffect(() => {
    shown.current = generation;
    busy.current = false;
  }, [generation]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const open = useCallback(async () => {
    if (!document || busy.current) return;
    busy.current = true;
    const asking = document.generation;
    let formats: ImageExportFormat[] = [];
    let message: string | null = document.info.textExtractionAllowed ? null : EXPORT_FORBIDDEN;
    try {
      formats = await engine.imageExportFormats();
      if (formats.length === 0) message ??= NO_FORMATS;
    } catch (error) {
      // A failed question refuses rather than offering formats the core may not have.
      message ??= sentence(errorMessage(error));
    }
    if (!mounted.current || shown.current !== asking) return;
    const format = formats.includes(DEFAULT_DRAFT.format) ? DEFAULT_DRAFT.format : (formats[0] ?? DEFAULT_DRAFT.format);
    setActivity({
      kind: 'choosing',
      draft: { ...DEFAULT_DRAFT, format },
      formats,
      allowed: message === null,
      message,
      checking: false,
    });
  }, [engine, document]);

  /** A changed choice clears the last refusal: it was about the old one. */
  const edit = useCallback((draft: ExportDraft) => {
    setActivity((a) => (a.kind === 'choosing' && a.allowed && !a.checking ? { ...a, draft, message: null } : a));
  }, []);

  const cancel = useCallback(() => {
    if (activity.kind !== 'choosing' || activity.checking) return;
    setActivity(IDLE);
    busy.current = false;
  }, [activity]);

  /** Cancel export: the page in hand still goes out, then the export stops. */
  const stop = useCallback(() => {
    stopRequested.current = true;
  }, []);

  /** Writes the plan's files from the open export; the caller closes it. */
  const run = useCallback(
    async (plan: ExportPlan, exportHandle: number, current: () => boolean) => {
      const { files, dpi, format } = plan;
      let written = 0;
      for (const [i, { pageIndex, fileName }] of files.entries()) {
        if (!current()) return;
        if (stopRequested.current) {
          setActivity({ kind: 'done', message: exportCancelled(written) });
          return;
        }
        setActivity({ kind: 'exporting', page: pageIndex + 1, index: i + 1, total: files.length });
        let bytes: Uint8Array;
        try {
          bytes = await engine.exportPageImage(exportHandle, pageIndex, dpi, format);
        } catch (error) {
          if (current()) setActivity({ kind: 'failed', message: pageFailed(pageIndex + 1, errorMessage(error), written) });
          return;
        }
        if (!current()) return;
        try {
          await delivery.deliver({ bytes, name: fileName, type: imageMediaType(format), destination: { kind: 'newFile' } });
        } catch {
          if (current()) setActivity({ kind: 'failed', message: writeFailed(fileName, written) });
          return;
        }
        written++;
      }
      if (!current()) return;
      // A cancel pressed during the last page came too late to save anything: it all went out.
      setActivity({ kind: 'done', message: exportDone(written, format) });
    },
    [engine, delivery],
  );

  const exportImages = useCallback(async () => {
    if (!document || activity.kind !== 'choosing' || !activity.allowed || activity.checking) return;
    const { draft } = activity;
    const early = precheckExport(draft, document.info.textExtractionAllowed);
    if (early !== null) {
      setActivity({ ...activity, message: early });
      return;
    }
    const running = document.generation;
    const current = () => mounted.current && shown.current === running;
    stopRequested.current = false;
    setActivity({ ...activity, checking: true, message: null });
    /** A refusal keeps the dialog open: the export is still the user's to fix. */
    let keepDialog = false;
    try {
      // The edits asked for before the click go in.
      await settleEdits?.();
      if (!current()) return;
      const opened = await engine.openImageExport(document.handle);
      try {
        if (!current()) return;
        const planned = await planImageExport(
          engine,
          { handle: document.handle, name: document.name, textExtractionAllowed: true, pageCount: opened.pageCount },
          draft,
          currentPage(),
        );
        if (!current()) return;
        if (planned.kind === 'refused') {
          keepDialog = true;
          // Only the message changes: the dialog stays open on what was typed.
          setActivity((a) => (a.kind === 'choosing' ? { ...a, checking: false, message: planned.message } : a));
          return;
        }
        await run(planned.plan, opened.handle, current);
      } finally {
        // The snapshot is the engine's memory: freed however the export ended.
        await engine.closeImageExport(opened.handle).catch(() => false);
      }
    } catch (error) {
      if (current()) setActivity({ kind: 'failed', message: openFailed(errorMessage(error)) });
    } finally {
      if (current() && !keepDialog) busy.current = false;
    }
  }, [engine, document, settleEdits, currentPage, activity, run]);

  return { activity, open, edit, cancel, stop, exportImages };
}
