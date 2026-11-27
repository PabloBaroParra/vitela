import { describe, expect, it } from 'vitest';
import { EngineError } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { DEFAULT_DRAFT, type ExportDraft } from './exportText';
import { planImageExport, precheckExport } from './planExport';

/** The snapshot's page count is what the plan counts: 5 here. */
const document = { handle: 7, name: 'report.pdf', textExtractionAllowed: true, pageCount: 5 };

/** An engine whose answers are the core's shape: names, a grammar, a ceiling over 300 DPI. */
function planningEngine() {
  const asked: string[] = [];
  const engine = fakeEngine({
    parsePageSelection: async (input, total) => {
      asked.push(`parse:${input}/${total}`);
      if (input === '9') throw new EngineError('InvalidPageSelection', 'page 9 is past the last page (5)');
      return input === '2-3' ? [1, 2] : [0];
    },
    firstPageTooLargeToExport: async (handle, pages, dpi) => (asked.push(`ceiling:${handle}:${pages}:${dpi}`), dpi > 300 ? pages[0]! : null),
    pageImageFileNames: async (name, pages, total, format) => (
      asked.push(`names:${name}:${total}:${format}`),
      pages.map((page) => `${name}-${page + 1}.${format}`)
    ),
  });
  return { engine, asked };
}

const draft = (change: Partial<ExportDraft>): ExportDraft => ({ ...DEFAULT_DRAFT, ...change });

describe('planImageExport', () => {
  it('starts with every page, PNG, at 150 DPI', () => {
    expect(DEFAULT_DRAFT).toEqual({ pages: 'all', customRange: '', dpi: '150', format: 'png' });
  });

  it('plans every page, named by the core', async () => {
    const { engine, asked } = planningEngine();
    const result = await planImageExport(engine, document, DEFAULT_DRAFT, 2);
    expect(result).toEqual({
      kind: 'plan',
      plan: {
        dpi: 150,
        format: 'png',
        files: [0, 1, 2, 3, 4].map((pageIndex) => ({ pageIndex, fileName: `report.pdf-${pageIndex + 1}.png` })),
      },
    });
    expect(asked).toEqual(['ceiling:7:0,1,2,3,4:150', 'names:report.pdf:5:png']);
  });

  it('plans the current page, clamped to the last one', async () => {
    const { engine } = planningEngine();
    const current = await planImageExport(engine, document, draft({ pages: 'current' }), 3);
    expect(current.kind === 'plan' && current.plan.files.map((file) => file.pageIndex)).toEqual([3]);
    const past = await planImageExport(engine, document, draft({ pages: 'current' }), 9);
    expect(past.kind === 'plan' && past.plan.files.map((file) => file.pageIndex)).toEqual([4]);
  });

  it('reads a custom range by the core\'s grammar, and shows its sentence when it cannot', async () => {
    const { engine, asked } = planningEngine();
    const range = await planImageExport(engine, document, draft({ pages: 'custom', customRange: '2-3', format: 'jpeg', dpi: '96' }), 0);
    expect(range).toEqual({
      kind: 'plan',
      plan: {
        dpi: 96,
        format: 'jpeg',
        files: [
          { pageIndex: 1, fileName: 'report.pdf-2.jpeg' },
          { pageIndex: 2, fileName: 'report.pdf-3.jpeg' },
        ],
      },
    });
    expect(asked[0]).toBe('parse:2-3/5');
    expect(await planImageExport(engine, document, draft({ pages: 'custom', customRange: '9' }), 0)).toEqual({
      kind: 'refused',
      message: 'Page 9 is past the last page (5).',
    });
  });

  it('refuses a resolution out of range before asking the engine anything', async () => {
    const { engine, asked } = planningEngine();
    expect(await planImageExport(engine, document, draft({ dpi: '600' }), 0)).toEqual({
      kind: 'refused',
      message: 'Choose a resolution between 72 and 400 DPI.',
    });
    expect(asked).toEqual([]);
  });

  it('names the first page too large at that resolution', async () => {
    const { engine } = planningEngine();
    expect(await planImageExport(engine, document, draft({ pages: 'custom', customRange: '2-3', dpi: '400' }), 0)).toEqual({
      kind: 'refused',
      message: 'Page 2 is too large to export at 400 DPI. Choose a lower resolution.',
    });
  });

  it('refuses a document that withholds extraction, and one with no pages', async () => {
    const { engine, asked } = planningEngine();
    expect(await planImageExport(engine, { ...document, pageCount: 2, textExtractionAllowed: false }, DEFAULT_DRAFT, 0)).toEqual({
      kind: 'refused',
      message: 'This document does not permit extracting its pages as images.',
    });
    expect(await planImageExport(engine, { ...document, pageCount: 0 }, DEFAULT_DRAFT, 0)).toEqual({
      kind: 'refused',
      message: 'This document has no pages to export.',
    });
    expect(asked).toEqual([]);
  });

  it('prechecks the choices that need no snapshot: the resolution, then the permission', () => {
    expect(precheckExport(DEFAULT_DRAFT, true)).toBeNull();
    expect(precheckExport(draft({ dpi: '600' }), false)).toBe('Choose a resolution between 72 and 400 DPI.');
    expect(precheckExport(DEFAULT_DRAFT, false)).toBe('This document does not permit extracting its pages as images.');
  });

  it('lets any other engine failure through', async () => {
    const engine = fakeEngine({
      firstPageTooLargeToExport: async () => {
        throw new EngineError('DocumentNotFound', 'gone');
      },
    });
    await expect(planImageExport(engine, document, DEFAULT_DRAFT, 0)).rejects.toMatchObject({ name: 'DocumentNotFound' });
  });
});
