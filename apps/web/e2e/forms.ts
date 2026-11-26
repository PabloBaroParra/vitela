// Shared steps of the forms E2E, and the signed form it fills: built in
// memory from the repo's signed fixture, never written to disk.

import { expect, type Locator, type Page } from '@playwright/test';
import { fixtureBytes, pageSlot } from './support';

export const SIGNED = 'signed/rsa2048_sha256.pdf';

export const formsPanel = (page: Page) => page.getByRole('complementary', { name: 'Form fields' });

/** Opens the forms panel and waits for its fields. */
export async function openForms(page: Page): Promise<Locator> {
  await page.getByRole('button', { name: 'Forms', exact: true }).click();
  const panel = formsPanel(page);
  await expect(panel.getByRole('status')).toHaveCount(0, { timeout: 30_000 });
  return panel;
}

/** A digest of page `number`'s pixels: it changes when pdfium paints anything else there. */
export function pagePixels(page: Page, number = 1): Promise<string> {
  return pageSlot(page, number)
    .locator('canvas.page-canvas')
    .evaluate(async (node) => {
      const canvas = node as HTMLCanvasElement;
      const { data } = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height);
      const digest = await crypto.subtle.digest('SHA-256', data);
      return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
    });
}

/**
 * `signed/rsa2048_sha256.pdf` with one more revision on top of its signed
 * bytes: a text field `signer_note` (holding "Pending") on its page. The
 * signature still covers the bytes it signed, so the document opens as
 * signed and every byte of the fixture is its prefix.
 *
 * The fixture's last revision (see its bytes): catalog 6 with `/AcroForm
 * << /Fields [9 0 R] /SigFlags 3 >>`, page 5, Helvetica font 2, the
 * signature widget 9, and a cross-reference stream at `startxref`.
 */
export function signedForm(): { readonly original: Buffer; readonly bytes: Buffer } {
  const original = fixtureBytes(SIGNED);
  const file = original.toString('latin1');
  const previous = Number(/startxref\s+(\d+)\s+%%EOF\s*$/.exec(file)![1]);
  const id = /\/ID\s*(\[[^\]]*\])/.exec(file)![1]!;

  let out = file.endsWith('\n') ? '' : '\n';
  const offsets = new Map<number, number>();
  const object = (number: number, body: string) => {
    offsets.set(number, original.length + Buffer.byteLength(out, 'latin1'));
    out += `${number} 0 obj\n${body}\nendobj\n`;
  };
  object(5, '<</Type/Page/Parent 1 0 R/Contents 4 0 R/Resources 3 0 R/MediaBox[0 0 612 792]/Annots[9 0 R 11 0 R]>>');
  object(
    6,
    '<</Type/Catalog/Pages 1 0 R/AcroForm<</Fields[9 0 R 11 0 R]/SigFlags 3/DA(/Helv 0 Tf 0 g)/DR<</Font<</Helv 2 0 R>>>>>>>>',
  );
  object(11, '<</Type/Annot/Subtype/Widget/FT/Tx/F 4/P 5 0 R/T(signer_note)/V(Pending)/Rect[72 560 372 590]/DA(/Helv 12 Tf 0 g)>>');

  // The cross-reference stream (object 12) lists 5, 6, 11 and itself.
  const xrefOffset = original.length + Buffer.byteLength(out, 'latin1');
  offsets.set(12, xrefOffset);
  const entry = (offset: number) => {
    const row = Buffer.alloc(7);
    row.writeUInt8(1, 0);
    row.writeUInt32BE(offset, 1);
    row.writeUInt16BE(0, 5);
    return row;
  };
  const rows = Buffer.concat([5, 6, 11, 12].map((number) => entry(offsets.get(number)!)));
  out +=
    `12 0 obj\n<</Type/XRef/Root 6 0 R/ID${id}/Size 13/Prev ${previous}/W[1 4 2]/Index[5 2 11 2]/Length ${rows.length}>>stream\n` +
    `${rows.toString('latin1')}\nendstream\nendobj\nstartxref\n${xrefOffset}\n%%EOF\n`;
  return { original, bytes: Buffer.concat([original, Buffer.from(out, 'latin1')]) };
}
