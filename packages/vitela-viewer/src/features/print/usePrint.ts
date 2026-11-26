// Print: what a save would write now, on paper, through the browser's print
// dialog. Like the native shells (GTK and Windows print an annotated session
// from its output snapshot), the pages are drawn from the engine's print
// snapshot, so pending page edits, annotations, fills and watermarks print;
// the on-screen preview leaves the session's annotations out.
//
// Print waits for the queued edits, opens the snapshot, checks no page is
// over the raster ceiling at `PRINT_DPI`, then draws one page at a time into
// an image (only the encoded file is kept; Cancel print stops after the page
// in hand). The snapshot is closed before the dialog opens, however the
// drawing ended. The sheets go into a hidden print frame inside the editor
// (`printFrame.ts`), which prints itself and is taken away, with its images,
// once the dialog closes — or by the next print, a new document or the
// editor going away, whichever comes first. No permission is asked: the core
// has no rule for the print bit and no native shell asks one. Container
// logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage, type Engine } from '@vitela/core';
import { sentence } from '../../ui/text';
import type { OpenDocument } from '../document/useDocument';
import { iframeSurface, type PrintJob, type PrintSurface } from './printFrame';
import { PRINT_DPI, pngSize, sheetOf, type PrintSheet } from './printSheets';
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
  /** The document on screen: a print for an old document is dropped. */
  const shown = useRef(generation);
  const mounted = useRef(true);
  const busy = useRef(false);
  /** Set by Cancel print: drawing stops after the page in hand. */
  const stopRequested = useRef(false);
  /** The frame handed to the print dialog, until it is taken away. */
  const printed = useRef<{ readonly job: PrintJob; readonly urls: readonly string[] } | null>(null);

  const takeAway = useCallback(() => {
    const last = printed.current;
    printed.current = null;
    if (!last) return;
    last.job.remove();
    for (const url of last.urls) URL.revokeObjectURL(url);
  }, []);

  // A new document starts clean (adjusting state while rendering).
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
  }
  useEffect(() => {
    shown.current = generation;
    busy.current = false;
    takeAway();
  }, [generation, takeAway]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      takeAway();
    };
  }, [takeAway]);

  const stop = useCallback(() => {
    stopRequested.current = true;
  }, []);

  /** Draws every page of the snapshot; `null` when the print was cancelled or dropped. */
  const draw = useCallback(
    async (snapshot: number, pageCount: number, current: () => boolean, urls: string[]): Promise<PrintSheet[] | null> => {
      const sheets: PrintSheet[] = [];
      for (let page = 0; page < pageCount; page++) {
        if (!current()) return null;
        if (stopRequested.current) {
          setActivity({ kind: 'done', message: PRINT_CANCELLED });
          return null;
        }
        setActivity({ kind: 'preparing', page: page + 1, total: pageCount });
        let bytes: Uint8Array;
        try {
          bytes = await engine.exportPageImage(snapshot, page, PRINT_DPI, 'png');
        } catch (error) {
          throw new Refused(pageFailed(page + 1, sentence(errorMessage(error))));
        }
        const size = pngSize(bytes);
        if (!size) throw new Refused(pageFailed(page + 1, 'The page was not drawn as an image.'));
        const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type: 'image/png' }));
        urls.push(url);
        sheets.push(sheetOf(url, size, PRINT_DPI));
      }
      return sheets;
    },
    [engine],
  );

  const print = useCallback(async () => {
    if (!document || busy.current) return;
    busy.current = true;
    // One print frame at a time: a dialog that never reported closing gives way.
    takeAway();
    const running = document.generation;
    const current = () => mounted.current && shown.current === running;
    stopRequested.current = false;
    setActivity({ kind: 'preparing', page: 0, total: document.info.pageCount });
    const urls: string[] = [];
    let handedOver = false;
    try {
      // The edits asked for before the click go in.
      await settleEdits();
      if (!current()) return;
      const opened = await engine.openPrintSnapshot(document.handle);
      let sheets: PrintSheet[] | null;
      try {
        if (opened.pageCount === 0) throw new Refused(NO_PAGES);
        const pages = Array.from({ length: opened.pageCount }, (_, page) => page);
        const oversized = await engine.firstPageTooLargeToExport(document.handle, pages, PRINT_DPI);
        if (oversized !== null) throw new Refused(pageTooLarge(oversized + 1));
        sheets = await draw(opened.handle, opened.pageCount, current, urls);
      } finally {
        // The snapshot is the engine's memory: freed before the dialog, however the drawing ended.
        await engine.closeImageExport(opened.handle).catch(() => false);
      }
      if (!sheets || !current()) return;
      const host = frameHost();
      if (!host) return;
      const job = await surface(host, titleOf(document.name), sheets);
      if (!current()) {
        job.remove();
        return;
      }
      const entry = { job, urls };
      printed.current = entry;
      handedOver = true;
      job.afterPrint(() => {
        if (printed.current === entry) takeAway();
      });
      setActivity(IDLE);
      job.print();
    } catch (error) {
      if (current()) {
        setActivity({ kind: 'failed', message: error instanceof Refused ? error.message : printFailed(sentence(errorMessage(error))) });
      }
    } finally {
      if (!handedOver) for (const url of urls) URL.revokeObjectURL(url);
      if (current()) busy.current = false;
    }
  }, [engine, document, settleEdits, frameHost, surface, draw, takeAway]);

  return { activity, print, stop };
}
