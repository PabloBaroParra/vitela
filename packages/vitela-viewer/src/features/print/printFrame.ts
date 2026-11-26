// Print, the browser side: a hidden frame holding only the sheets, printed
// through its own window. Printing the frame instead of the page means an
// embedding host's page never prints (only the editor's pages do), and the
// frame's document has its own stylesheet, so neither side's styles reach
// the other. Native printing stays the browser's: its dialog picks the
// printer, the copies and the paper. I/O adapter; `printSheets.ts` writes
// the document.

import { writePrintDocument, type PrintSheet } from './printSheets';

/** A print frame, filled and ready. */
export interface PrintJob {
  /** Opens the browser's print dialog over the frame's sheets. */
  print(): void;
  /** Called once the dialog closes, printed or not. */
  afterPrint(listener: () => void): void;
  /** Takes the frame out of the page. */
  remove(): void;
}

/** Builds a print frame for `sheets` inside `host`, titled `title`. */
export type PrintSurface = (host: HTMLElement, title: string, sheets: readonly PrintSheet[]) => Promise<PrintJob>;

/** Hidden, but laid out (a `display: none` frame does not print in Firefox). */
const FRAME_STYLE = 'position: absolute; width: 0; height: 0; border: 0; visibility: hidden;';
const EMPTY = '<!doctype html><html><head><meta charset="utf-8"></head><body></body></html>';

export const iframeSurface: PrintSurface = async (host, title, sheets) => {
  const frame = host.ownerDocument.createElement('iframe');
  frame.className = 'vitela-print-frame';
  frame.title = 'Print';
  frame.tabIndex = -1;
  frame.setAttribute('aria-hidden', 'true');
  frame.setAttribute('style', FRAME_STYLE);
  // `srcdoc`, not a bare frame: Firefox swaps a bare frame's first document
  // for a fresh one when it loads, dropping whatever was written into it.
  const loaded = new Promise<void>((resolve) => frame.addEventListener('load', () => resolve(), { once: true }));
  frame.srcdoc = EMPTY;
  host.append(frame);
  await loaded;
  const view = frame.contentWindow;
  const doc = frame.contentDocument;
  if (!view || !doc) {
    frame.remove();
    throw new Error('the print frame could not be opened');
  }
  const images = writePrintDocument(doc, title, sheets);
  // Every page decoded before the dialog lays them out; one that fails still prints its alt text.
  await Promise.all(images.map((image) => image.decode().catch(() => undefined)));
  return {
    print: () => view.print(),
    afterPrint: (listener) => view.addEventListener('afterprint', () => listener(), { once: true }),
    remove: () => frame.remove(),
  };
};
