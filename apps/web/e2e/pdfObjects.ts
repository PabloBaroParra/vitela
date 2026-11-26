// Reads saved PDF bytes back in the E2E specs, just far enough to find
// annotation and page dictionaries: in the file's plain objects and inside its
// Flate-compressed streams (object streams), without a PDF library.

import { inflateSync } from 'node:zlib';

/** The bytes of every stream that inflates, plus the file itself. */
function texts(bytes: Buffer): string[] {
  const file = bytes.toString('latin1');
  const out = [file];
  const stream = /stream\r?\n/g;
  for (let match = stream.exec(file); match; match = stream.exec(file)) {
    const start = match.index + match[0].length;
    const end = file.indexOf('endstream', start);
    if (end < 0) break;
    try {
      out.push(inflateSync(bytes.subarray(start, end)).toString('latin1'));
    } catch {
      // Not Flate (or not a stream we can read): its dictionaries are not in it.
    }
    stream.lastIndex = end;
  }
  return out;
}

/** The `<< … >>` dictionary around position `at` of `text`. */
function enclosingDictionary(text: string, at: number): string | null {
  let depth = 0;
  let start = -1;
  for (let i = at; i > 0; i--) {
    const pair = text.slice(i - 1, i + 1);
    if (pair === '>>') {
      depth++;
      i--;
    } else if (pair === '<<') {
      if (depth === 0) {
        start = i - 1;
        break;
      }
      depth--;
      i--;
    }
  }
  if (start < 0) return null;
  depth = 0;
  for (let i = start; i < text.length - 1; i++) {
    const pair = text.slice(i, i + 2);
    if (pair === '<<') {
      depth++;
      i++;
    } else if (pair === '>>') {
      depth--;
      i++;
      if (depth === 0) return text.slice(start, i + 1);
    }
  }
  return null;
}

/**
 * The `/Rect` (x0, y0, x1, y1 in PDF points, origin bottom-left) of every
 * annotation of `subtype` (`Highlight`, `Underline`, `StrikeOut`, …) the
 * saved bytes hold.
 */
export function annotationRects(bytes: Buffer, subtype: string): number[][] {
  const rects: number[][] = [];
  for (const dictionary of subtypeDictionaries(bytes, subtype)) {
    const rect = /\/Rect\s*\[([^\]]*)\]/.exec(dictionary);
    if (rect) rects.push(rect[1]!.trim().split(/\s+/).map(Number));
  }
  return rects;
}

/**
 * Every dictionary of `subtype` the saved bytes hold, as its source text:
 * an annotation's (`Stamp`, …) or an XObject's (`Image`, `Form`).
 */
export function subtypeDictionaries(bytes: Buffer, subtype: string): string[] {
  const found: string[] = [];
  const pattern = new RegExp(`/Subtype\\s*/${subtype}(?![A-Za-z0-9])`, 'g');
  for (const text of texts(bytes)) {
    for (let match = pattern.exec(text); match; match = pattern.exec(text)) {
      const dictionary = enclosingDictionary(text, match.index);
      if (dictionary) found.push(dictionary);
    }
  }
  return found;
}

/**
 * The `/MediaBox` of every page dictionary (`/Type /Page`) the saved bytes
 * hold, in file order (not page order), as x0, y0, x1, y1 in PDF points.
 * A page that inherits its box from `/Pages` is not listed.
 */
export function pageMediaBoxes(bytes: Buffer): number[][] {
  const boxes: number[][] = [];
  const pattern = /\/Type\s*\/Page(?![A-Za-z0-9])/g;
  for (const text of texts(bytes)) {
    for (let match = pattern.exec(text); match; match = pattern.exec(text)) {
      const dictionary = enclosingDictionary(text, match.index);
      const box = dictionary && /\/MediaBox\s*\[([^\]]*)\]/.exec(dictionary);
      if (box) boxes.push(box[1]!.trim().split(/\s+/).map(Number));
    }
  }
  return boxes;
}
