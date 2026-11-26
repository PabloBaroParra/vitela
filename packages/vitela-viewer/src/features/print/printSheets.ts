// Print, the sheets: what each drawn page is on paper, and the print-only
// document that holds them. One sheet per page, in order, each its own size
// and orientation (a page turned a quarter is drawn landscape by the core, so
// its sheet is landscape too). The document is the print frame's own: no
// stylesheet, script or markup of the editor's or the host's goes into it,
// and nothing of it leaks out. Pure apart from the DOM it is handed.

/**
 * The resolution pages are drawn at for printing. The GTK shell rasterizes at
 * 300; the web holds every page as an image until the print dialog closes,
 * and 150 DPI keeps a Letter page at ~2 Mpx (a quarter of 300's) while text
 * still prints sharp.
 */
export const PRINT_DPI = 150;

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
 * Fills `doc` (the print frame's document) with `sheets`, replacing whatever
 * it held, and titles it `title` (the name the print dialog offers when it
 * saves to a file). Returns the images, in page order, for the caller to wait
 * on before printing.
 */
export function writePrintDocument(doc: Document, title: string, sheets: readonly PrintSheet[]): HTMLImageElement[] {
  doc.title = title;
  for (const style of doc.head.querySelectorAll('style')) style.remove();
  const style = doc.createElement('style');
  style.textContent = printStyles(sheets);
  doc.head.append(style);

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
