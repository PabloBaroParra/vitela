// Builds small, valid PDFs in memory for flows no committed fixture covers
// (the repo's fixtures have at most three pages, none rotated, none that
// withholds its text). Nothing is written to disk: the bytes go straight to
// the file input or the drop event.

import { createHash } from 'node:crypto';

export interface PageSpec {
  readonly widthPt: number;
  readonly heightPt: number;
  readonly rotate?: 0 | 90 | 180 | 270;
  /** Lines of text drawn under the band, top to bottom (see `lineBaseline`). */
  readonly lines?: readonly string[];
}

export interface PdfOptions {
  /**
   * Encrypts the document with the standard security handler (RC4, 128-bit,
   * revision 3) under `userPassword`, granting only `permissions` (the `/P`
   * value) to whoever opens it with that password.
   *
   * Non-empty on purpose: the core's open path does not yet read the
   * permissions of a document whose user password is empty (lopdf decrypts
   * it on load and drops `/Encrypt`), so such a document is treated as
   * unrestricted — a known core gap, see docs/batch-web.md (T-222).
   */
  readonly encrypt?: { readonly permissions: number; readonly userPassword: string };
}

/** `/P` granting everything but bit 5, copying or extracting text. */
export const COPY_PROHIBITED = -4 & ~(1 << 4);

/** Size of the `lines` text, in points. */
export const LINE_FONT_PT = 18;

/** The baseline of line `index` of `PageSpec.lines`, in PDF points from the bottom. */
export function lineBaseline(page: PageSpec, index: number): number {
  return page.heightPt - 160 - index * 30;
}

/**
 * One page per spec, each with a dark band and its number in Helvetica, so
 * every page has ink a test can look for, plus its `lines`.
 */
export function buildPdf(pages: readonly PageSpec[], options: PdfOptions = {}): Buffer {
  const objects: { body: string; stream?: string }[] = [];
  // Object number n is objects[n - 1].
  const add = (body: string, stream?: string) => {
    objects.push({ body, stream });
    return objects.length;
  };
  // 1: catalog, 2: page tree, 3: font; pages follow in (page, contents) pairs.
  add('<< /Type /Catalog /Pages 2 0 R >>');
  add(''); // the page tree, filled once the kids are known
  add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  const kids: number[] = [];
  pages.forEach((page, index) => {
    const content = [
      '0.2 0.2 0.2 rg',
      `36 ${page.heightPt - 108} ${page.widthPt - 72} 72 re f`,
      `BT /F1 48 Tf 72 ${page.heightPt / 2} Td (Page ${index + 1}) Tj ET`,
      ...(page.lines ?? []).map(
        (line, row) => `BT /F1 ${LINE_FONT_PT} Tf 72 ${lineBaseline(page, row)} Td (${escape(line)}) Tj ET`,
      ),
    ].join('\n');
    const pageNumber = objects.length + 1;
    kids.push(pageNumber);
    add(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${page.widthPt} ${page.heightPt}]` +
        `${page.rotate ? ` /Rotate ${page.rotate}` : ''}` +
        ` /Resources << /Font << /F1 3 0 R >> >> /Contents ${pageNumber + 1} 0 R >>`,
    );
    add('', content);
  });
  objects[1] = { body: `<< /Type /Pages /Kids [${kids.map((kid) => `${kid} 0 R`).join(' ')}] /Count ${kids.length} >>` };

  const id = createHash('md5').update(`vitela-e2e ${pages.length}`).digest();
  const security = options.encrypt ? standardSecurity(options.encrypt, id) : null;
  const encryptRef = security ? add(security.dictionary) : null;

  let out = '%PDF-1.7\n';
  const offsets: number[] = [];
  objects.forEach(({ body, stream }, index) => {
    offsets.push(out.length);
    const number = index + 1;
    if (stream === undefined) {
      out += `${number} 0 obj\n${body}\nendobj\n`;
      return;
    }
    let data: Buffer = Buffer.from(stream, 'latin1');
    if (security) data = security.encrypt(number, data);
    out += `${number} 0 obj\n<< /Length ${data.length} >>\nstream\n${data.toString('latin1')}\nendstream\nendobj\n`;
  });
  const xref = out.length;
  out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  out += offsets.map((offset) => `${String(offset).padStart(10, '0')} 00000 n \n`).join('');
  const hexId = id.toString('hex');
  out +=
    `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R /ID [<${hexId}> <${hexId}>]` +
    `${encryptRef ? ` /Encrypt ${encryptRef} 0 R` : ''} >>\n` +
    `startxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
}

function escape(text: string): string {
  return text.replace(/[\\()]/g, (char) => `\\${char}`);
}

// ---- Standard security handler, RC4 128-bit, revision 3 (PDF 1.7 §7.6.3) ----

const PASSWORD_PADDING = Buffer.from('28BF4E5E4E758A4164004E56FFFA01082E2E00B6D0683E802F0CA9FE6453697A', 'hex');
const KEY_BYTES = 16;
/** Never typed by the tests: the owner password would lift the restriction. */
const OWNER_PASSWORD = 'vitela-e2e-owner';

function standardSecurity({ permissions, userPassword }: { permissions: number; userPassword: string }, id: Buffer) {
  const user = padded(userPassword);
  // Algorithm 3: the /O entry.
  let ownerKey = md5(padded(OWNER_PASSWORD));
  for (let i = 0; i < 50; i++) ownerKey = md5(ownerKey);
  let owner = rc4(ownerKey, user);
  for (let i = 1; i <= 19; i++) owner = rc4(xor(ownerKey, i), owner);
  // Algorithm 2: the file key.
  const p = Buffer.alloc(4);
  p.writeInt32LE(permissions);
  let key = md5(user, owner, p, id);
  for (let i = 0; i < 50; i++) key = md5(key.subarray(0, KEY_BYTES));
  key = key.subarray(0, KEY_BYTES);
  // Algorithm 5: the /U entry.
  let check = rc4(key, md5(PASSWORD_PADDING, id));
  for (let i = 1; i <= 19; i++) check = rc4(xor(key, i), check);
  const userEntry = Buffer.concat([check, Buffer.alloc(16)]);

  return {
    dictionary:
      `<< /Filter /Standard /V 2 /R 3 /Length 128 /P ${permissions}` +
      ` /O <${owner.toString('hex')}> /U <${userEntry.toString('hex')}> >>`,
    /** Algorithm 1: each object's data under its own key. */
    encrypt(objectNumber: number, data: Buffer): Buffer {
      const salt = Buffer.from([objectNumber & 0xff, (objectNumber >> 8) & 0xff, (objectNumber >> 16) & 0xff, 0, 0]);
      return rc4(md5(key, salt).subarray(0, Math.min(KEY_BYTES + 5, 16)), data);
    },
  };
}

function padded(password: string): Buffer {
  return Buffer.concat([Buffer.from(password, 'latin1'), PASSWORD_PADDING]).subarray(0, 32);
}

function md5(...parts: Buffer[]): Buffer {
  const hash = createHash('md5');
  for (const part of parts) hash.update(part);
  return hash.digest();
}

function xor(key: Buffer, value: number): Buffer {
  return Buffer.from(key.map((byte) => byte ^ value));
}

function rc4(key: Buffer, data: Buffer): Buffer {
  const s = Array.from({ length: 256 }, (_, i) => i);
  for (let i = 0, j = 0; i < 256; i++) {
    j = (j + s[i]! + key[i % key.length]!) & 0xff;
    [s[i], s[j]] = [s[j]!, s[i]!];
  }
  const out = Buffer.alloc(data.length);
  for (let n = 0, i = 0, j = 0; n < data.length; n++) {
    i = (i + 1) & 0xff;
    j = (j + s[i]!) & 0xff;
    [s[i], s[j]] = [s[j]!, s[i]!];
    out[n] = data[n]! ^ s[(s[i]! + s[j]!) & 0xff]!;
  }
  return out;
}
