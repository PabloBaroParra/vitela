// Builds small, valid PDFs in memory for flows no committed fixture covers
// (the repo's fixtures have at most three pages). Nothing is written to
// disk: the bytes go straight to the file input or the drop event.

export interface PageSpec {
  readonly widthPt: number;
  readonly heightPt: number;
  readonly rotate?: 0 | 90 | 180 | 270;
}

/**
 * One page per spec, each with a dark band and its number in Helvetica, so
 * every page has ink a test can look for.
 */
export function buildPdf(pages: readonly PageSpec[]): Buffer {
  const objects: string[] = [];
  // Object number n is objects[n - 1].
  const add = (body: string) => {
    objects.push(body);
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
    ].join('\n');
    const pageNumber = objects.length + 1;
    kids.push(pageNumber);
    add(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${page.widthPt} ${page.heightPt}]` +
        `${page.rotate ? ` /Rotate ${page.rotate}` : ''}` +
        ` /Resources << /Font << /F1 3 0 R >> >> /Contents ${pageNumber + 1} 0 R >>`,
    );
    add(`<< /Length ${content.length} >>\nstream\n${content}\nendstream`);
  });
  objects[1] = `<< /Type /Pages /Kids [${kids.map((kid) => `${kid} 0 R`).join(' ')}] /Count ${kids.length} >>`;

  let out = '%PDF-1.7\n';
  const offsets: number[] = [];
  objects.forEach((body, index) => {
    offsets.push(out.length);
    out += `${index + 1} 0 obj\n${body}\nendobj\n`;
  });
  const xref = out.length;
  out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  out += offsets.map((offset) => `${String(offset).padStart(10, '0')} 00000 n \n`).join('');
  out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
}
