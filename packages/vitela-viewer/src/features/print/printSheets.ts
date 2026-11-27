// Print, the sheets: what each drawn page is on paper, and the print-only
// document that holds them. One sheet per page, in order, each its own size
// and orientation (a page turned a quarter is drawn landscape by the core, so
// its sheet is landscape too). The document is the print frame's own: no
// stylesheet, script or markup of the editor's or the host's goes into it,
// and nothing of it leaks out. Pure apart from the DOM it is handed.

/**
 * The resolution pages are drawn at for printing, at best. The GTK shell
 * rasterizes at 300; the web holds every page as an image until the print
 * dialog closes, and 150 DPI keeps a Letter page at ~2 Mpx (a quarter of
 * 300's) while text still prints sharp. It is also the core's cap for a
 * print snapshot (`PRINT_MAX_DPI` in `pdf-wasm`): never draw finer.
 */
export const PRINT_DPI = 150;

/** The lowest resolution a long document is lowered to: text still reads on paper. */
export const PRINT_MIN_DPI = 96;

/**
 * How many pixels every page of one print may add up to before the
 * resolution is lowered: the print frame keeps each page decoded (4 bytes a
 * pixel) until the dialog closes, so ~512 MB — about 60 Letter pages at 150
 * DPI, 148 at the floor. Past the floor a document still prints, at 96 DPI.
 */
export const PRINT_PIXEL_BUDGET = 128_000_000;

/** A page's size, in points. */
export interface PagePoints {
  readonly widthPt: number;
  readonly heightPt: number;
}

/** US Letter, for pages whose size is not known. */
const LETTER_SQUARE_INCHES = (612 / 72) * (792 / 72);

/**
 * The resolution to print `pageCount` pages at: `PRINT_DPI` while they fit
 * `PRINT_PIXEL_BUDGET`, else the finest that fits, never below
 * `PRINT_MIN_DPI`. `sizes` are the pages as last seen (the snapshot may hold
 * more after a pending edit); a page with no size counts as the average of
 * those known, or Letter when none is.
 */
export function printDpi(sizes: readonly PagePoints[], pageCount: number): number {
  const known = sizes.slice(0, pageCount).map(({ widthPt, heightPt }) => (widthPt / 72) * (heightPt / 72));
  const knownArea = known.reduce((sum, area) => sum + area, 0);
  const average = known.length > 0 ? knownArea / known.length : LETTER_SQUARE_INCHES;
  const squareInches = knownArea + (pageCount - known.length) * average;
  if (squareInches <= 0) return PRINT_DPI;
  const fits = Math.floor(Math.sqrt(PRINT_PIXEL_BUDGET / squareInches));
  return Math.max(PRINT_MIN_DPI, Math.min(PRINT_DPI, fits));
}

/** A drawn page, in pixels. */
export interface PixelSize {
  readonly width: number;
  readonly height: number;
}

/** One page to print: its image and the size of its sheet, in points. */
export interface PrintSheet {
  readonly url: string;
  readonly widthPt: number;
  readonly heightPt: number;
}

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
const IHDR = [0x49, 0x48, 0x44, 0x52];

/** A PNG's width and height, read from its header; `null` for anything else. */
export function pngSize(bytes: Uint8Array): PixelSize | null {
  if (bytes.length < 24) return null;
  if (PNG_SIGNATURE.some((byte, i) => bytes[i] !== byte)) return null;
  if (IHDR.some((byte, i) => bytes[12 + i] !== byte)) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, 24);
  return { width: view.getUint32(16), height: view.getUint32(20) };
}

/** Rounded to a hundredth of a point: pixels over a DPI are rarely whole. */
const points = (pixels: number, dpi: number) => Math.round(((pixels * 72) / dpi) * 100) / 100;

/** The sheet of a page drawn as `size` pixels at `dpi`. */
export function sheetOf(url: string, size: PixelSize, dpi: number): PrintSheet {
  return { url, widthPt: points(size.width, dpi), heightPt: points(size.height, dpi) };
}

const sizeKey = (sheet: PrintSheet) => `${sheet.widthPt}x${sheet.heightPt}`;

/** Which named page size each sheet takes: one per distinct size, in order of first use. */
function sizeIndexes(sheets: readonly PrintSheet[]): { readonly sizes: readonly PrintSheet[]; readonly of: readonly number[] } {
  const known = new Map<string, number>();
  const sizes: PrintSheet[] = [];
  const of = sheets.map((sheet) => {
    const key = sizeKey(sheet);
    let index = known.get(key);
    if (index === undefined) {
      index = sizes.length;
      known.set(key, index);
      sizes.push(sheet);
    }
    return index;
  });
  return { sizes, of };
}

/**
 * The print document's stylesheet. Each distinct size is a named `@page`
 * (CSS Paged Media), so the browser offers it as the paper: a landscape page
 * prints on a landscape sheet. A browser that ignores named sizes still
 * breaks after every page, and the image keeps its proportions.
 */
export function printStyles(sheets: readonly PrintSheet[]): string {
  const { sizes } = sizeIndexes(sheets);
  return [
    '@page { margin: 0; }',
    ...sizes.map((size, i) => `@page vitela-sheet-${i} { size: ${size.widthPt}pt ${size.heightPt}pt; margin: 0; }`),
    'html, body { margin: 0; padding: 0; background: #fff; }',
    '.sheet { display: block; overflow: hidden; break-inside: avoid; break-after: page; }',
    '.sheet:last-child { break-after: auto; }',
    '.sheet img { display: block; width: 100%; height: 100%; object-fit: contain; }',
    ...sizes.map((size, i) => `.sheet--${i} { page: vitela-sheet-${i}; width: ${size.widthPt}pt; height: ${size.heightPt}pt; }`),
  ].join('\n');
}

/**
 * Makes `css` the only stylesheet of `doc`. A constructed stylesheet where
 * the browser has them: a `<style>` element is inline style, which a strict
 * Content-Security-Policy refuses (the Vitela web app's does), and the print
 * frame, a `srcdoc`, runs under the policy of the page that holds it. The
 * sheet must come from `doc`'s own window to be adopted there. Without
 * constructed stylesheets (older browsers, jsdom) it is a `<style>`.
 */
export function applyStylesheet(doc: Document, css: string): void {
  for (const style of doc.head.querySelectorAll('style')) style.remove();
  const Sheet = doc.defaultView?.CSSStyleSheet;
  if (Sheet && 'adoptedStyleSheets' in doc && typeof Sheet.prototype.replaceSync === 'function') {
    const sheet = new Sheet();
    sheet.replaceSync(css);
    doc.adoptedStyleSheets = [sheet];
    return;
  }
  const style = doc.createElement('style');
  style.textContent = css;
  doc.head.append(style);
}

/**
 * Fills `doc` (the print frame's document) with `sheets`, replacing whatever
 * it held, and titles it `title` (the name the print dialog offers when it
 * saves to a file). Returns the images, in page order, for the caller to wait
 * on before printing.
 */
export function writePrintDocument(doc: Document, title: string, sheets: readonly PrintSheet[]): HTMLImageElement[] {
  doc.title = title;
  applyStylesheet(doc, printStyles(sheets));

  const { of } = sizeIndexes(sheets);
  const images = sheets.map((sheet, i) => {
    const image = doc.createElement('img');
    image.src = sheet.url;
    image.alt = `Page ${i + 1}`;
    return image;
  });
  doc.body.replaceChildren(
    ...images.map((image, i) => {
      const wrap = doc.createElement('div');
      wrap.className = `sheet sheet--${of[i]}`;
      wrap.append(image);
      return wrap;
    }),
  );
  return images;
}
