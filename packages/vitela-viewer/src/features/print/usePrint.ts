// Print: what a save would write now, on paper, through the browser's print
// dialog. Like the native shells (GTK and Windows print an annotated session
// from its output snapshot), the pages are drawn from the engine's print
// snapshot, so pending page edits, annotations, fills and watermarks print;
// the on-screen preview leaves the session's annotations out.
//
// Print waits for the queued edits, opens the snapshot, picks the resolution
// (`printDpi`: 150 DPI, lowered for a long document so every page together
// stays within a pixel budget, never below 96), checks no page is over the
// raster ceiling at it, then draws one page at a time into an image (only the
// encoded file is kept). The snapshot lifecycle and the drawing loop are the
// shared snapshot job's (`features/snapshot`), as for Export images. The
// snapshot is closed before the dialog opens, however the drawing ended.
// Cancel print is heard before each page, after the last one, and while the
// frame gets its pages ready: until the dialog opens, a cancel means no
// dialog and nothing kept. The sheets go into a hidden print frame inside the
// editor (`printFrame.ts`), which prints itself and is taken away, with its
// images, once the dialog closes — or by the next print, a new document or
// the editor going away, whichever comes first. No permission is asked: the
// core has no rule for the print bit and no native shell asks one. Container
// logic only, no markup.

import { useCallback, useRef, useState } from 'react';
import { errorMessage, type Engine } from '@vitela/core';
import { sentence } from '../../ui/text';
import type { OpenDocument } from '../document/useDocument';
import { PageNotDrawn, drawPages, withSnapshot } from '../snapshot/snapshotJob';
import { useSnapshotJob } from '../snapshot/useSnapshotJob';
import { iframeSurface, type PrintJob, type PrintSurface } from './printFrame';
import { pngSize, printDpi, sheetOf, type PrintSheet } from './printSheets';
import { NO_PAGES, PRINT_CANCELLED, pageFailed, pageTooLarge, printFailed } from './printText';

export type PrintActivity =
  | { readonly kind: 'idle' }
  /** Drawing the pages; `page` is one-based, 0 before the first. */
  | { readonly kind: 'preparing'; readonly page: number; readonly total: number }
  | { readonly kind: 'done'; readonly message: string }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: PrintActivity = { kind: 'idle' };

/** While the pages are being drawn, Print cannot be started again. */
export function printBusy(activity: PrintActivity): boolean {
  return activity.kind === 'preparing';
}

/** What the workspace lends Print: the edit queue and the place for the frame. */
export interface PrintHost {
  /** Settles once the edits the editing screen has asked for so far have ended. */
  readonly settleEdits: () => Promise<unknown>;
  /** Where the print frame goes: inside the editor, never elsewhere on a host's page. */
  readonly frameHost: () => HTMLElement | null;
  /** How the sheets reach the print dialog (default: a hidden frame). */
  readonly surface?: PrintSurface;
}

export type PrintSession = ReturnType<typeof usePrint>;

/** The name the print dialog offers when it saves to a file: the document's, without `.pdf`. */
const titleOf = (name: string) => name.replace(/\.pdf$/i, '') || name;

class Refused extends Error {}

export function usePrint(engine: Engine, document: OpenDocument | null, { settleEdits, frameHost, surface = iframeSurface }: PrintHost) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<PrintActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The frame handed to the print dialog, until it is taken away. */
  const printed = useRef<{ readonly job: PrintJob; readonly urls: readonly string[] } | null>(null);

  const takeAway = useCallback(() => {
    const last = printed.current;
    printed.current = null;
    if (!last) return;
    last.job.remove();
    for (const url of last.urls) URL.revokeObjectURL(url);
  }, []);

  // A new document starts clean (adjusting state while rendering); the frame goes with the old one.
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
  }
  const job = useSnapshotJob(generation, takeAway);

  const print = useCallback(async () => {
    if (!document || !job.claim()) return;
    // One print frame at a time: a dialog that never reported closing gives way.
    takeAway();
    const guard = job.guard(document.generation);
    const cancelled = () => setActivity({ kind: 'done', message: PRINT_CANCELLED });
    setActivity({ kind: 'preparing', page: 0, total: document.info.pageCount });
    const urls: string[] = [];
    let handedOver = false;
    try {
      // The snapshot is closed before the frame is built, however the drawing ended.
      const drawn = await withSnapshot(
        engine,
        guard,
        settleEdits,
        () => engine.openPrintSnapshot(document.handle),
        async ({ handle, pageCount }) => {
          if (pageCount === 0) throw new Refused(NO_PAGES);
          const pages = Array.from({ length: pageCount }, (_, page) => page);
          const dpi = printDpi(document.info.pages, pageCount);
          const oversized = await engine.firstPageTooLargeToExport(document.handle, pages, dpi);
          if (oversized !== null) throw new Refused(pageTooLarge(oversized + 1));
          const sheets: PrintSheet[] = [];
          const end = await drawPages(engine, handle, pages, dpi, 'png', guard, {
            starting: (page) => setActivity({ kind: 'preparing', page: page + 1, total: pageCount }),
            drawn: (bytes, page) => {
              const size = pngSize(bytes);
              if (!size) throw new Refused(pageFailed(page + 1, 'The page was not drawn as an image.'));
              const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type: 'image/png' }));
              urls.push(url);
              sheets.push(sheetOf(url, size, dpi));
            },
          });
          return end === 'finished' ? sheets : end;
        },
      );
      if (drawn === undefined || drawn === 'dropped' || !guard.current()) return;
      // A cancel pressed while the last page was drawn still stops the print.
      if (drawn === 'stopped' || guard.stopped()) return cancelled();
      const host = frameHost();
      if (!host) return;
      const frame = await surface(host, titleOf(document.name), drawn, guard.stopped);
      if (!guard.current()) return frame.remove();
      // A cancel pressed while the frame got its pages ready: no dialog.
      if (guard.stopped()) {
        frame.remove();
        return cancelled();
      }
      const entry = { job: frame, urls };
      printed.current = entry;
      handedOver = true;
      frame.afterPrint(() => {
        if (printed.current === entry) takeAway();
      });
      setActivity(IDLE);
      frame.print();
    } catch (error) {
      if (guard.current()) setActivity({ kind: 'failed', message: failureOf(error) });
    } finally {
      if (!handedOver) for (const url of urls) URL.revokeObjectURL(url);
      if (guard.current()) job.release();
    }
  }, [engine, document, settleEdits, frameHost, surface, takeAway, job]);

  return { activity, print, stop: job.stop };
}

function failureOf(error: unknown): string {
  if (error instanceof Refused) return error.message;
  if (error instanceof PageNotDrawn) return pageFailed(error.pageIndex + 1, sentence(errorMessage(error.cause)));
  return printFailed(sentence(errorMessage(error)));
}
