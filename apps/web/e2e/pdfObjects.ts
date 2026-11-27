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

/**
 * The text keys of the saved document's `/Info` dictionary (`Title`,
 * `Author`, …), decoded: a literal string with its escapes, or a hex one
 * (UTF-16BE behind a byte-order mark). Found through the trailer's `/Info`
 * reference, as a plain object or inside an object stream. `{}` when there
 * is none.
 */
export function documentInfo(bytes: Buffer): Record<string, string> {
  const all = texts(bytes);
  const reference = all.map((text) => /\/Info\s+(\d+)\s+(\d+)\s+R/.exec(text)).find((match) => match);
  if (!reference) return {};
  const number = Number(reference[1]);
  const header = new RegExp(`(?:^|\\s)${number}\\s+${reference[2]}\\s+obj\\b`).exec(all[0]!);
  const dictionary = header
    ? enclosingDictionary(all[0]!, all[0]!.indexOf('<<', header.index) + 2)
    : objectStreamMember(bytes, number);
  if (!dictionary) return {};
  const keys: Record<string, string> = {};
  const entry = /\/([A-Za-z]+)\s*(\((?:\\.|[^\\)])*\)|<[0-9A-Fa-f\s]*>)/g;
  for (let match = entry.exec(dictionary); match; match = entry.exec(dictionary)) {
    keys[match[1]!] = decodeString(match[2]!);
  }
  return keys;
}

/**
 * How many times `needle` appears in the saved file, its Flate streams
 * inflated: a watermark's text shown by `Tj` on each page it stamped, or the
 * image name each of them paints with `Do`.
 */
export function occurrences(bytes: Buffer, needle: string): number {
  return texts(bytes).reduce((count, text) => count + text.split(needle).length - 1, 0);
}

/** The dictionary of object `number` when it lives in an object stream (`/Type /ObjStm`). */
function objectStreamMember(bytes: Buffer, number: number): string | null {
  const file = bytes.toString('latin1');
  const stream = /stream\r?\n/g;
  for (let match = stream.exec(file); match; match = stream.exec(file)) {
    const start = match.index + match[0].length;
    const end = file.indexOf('endstream', start);
    if (end < 0) break;
    stream.lastIndex = end;
    // The stream's own dictionary: the first `<<` after its `obj` header.
    const head = enclosingDictionary(file, file.indexOf('<<', file.lastIndexOf(' obj', match.index)) + 2) ?? '';
    const first = /\/First\s+(\d+)/.exec(head);
    if (!/\/Type\s*\/ObjStm/.test(head) || !first) continue;
    let body: string;
    try {
      body = inflateSync(bytes.subarray(start, end)).toString('latin1');
    } catch {
      continue;
    }
    const offsets = Number(first[1]);
    const pairs = body.slice(0, offsets).trim().split(/\s+/).map(Number);
    for (let i = 0; i + 1 < pairs.length; i += 2) {
      if (pairs[i] !== number) continue;
      return enclosingDictionary(body, body.indexOf('<<', offsets + pairs[i + 1]!) + 2);
    }
  }
  return null;
}

/** A PDF string token, `(…)` or `<…>`, as text. */
function decodeString(token: string): string {
  let raw: Buffer;
  if (token.startsWith('<')) {
    raw = Buffer.from(token.slice(1, -1).replace(/\s/g, ''), 'hex');
  } else {
    const escapes: Record<string, string> = { n: '\n', r: '\r', t: '\t', b: '\b', f: '\f' };
    const text = token
      .slice(1, -1)
      .replace(/\\([0-7]{1,3}|.)/g, (_, code: string) =>
        /^[0-7]/.test(code) ? String.fromCharCode(parseInt(code, 8)) : (escapes[code] ?? code),
      );
    raw = Buffer.from(text, 'latin1');
  }
  if (raw[0] === 0xfe && raw[1] === 0xff) {
    let out = '';
    for (let i = 2; i + 1 < raw.length; i += 2) out += String.fromCharCode(raw.readUInt16BE(i));
    return out;
  }
  return raw.toString('latin1');
}
