// What one card of the Organize grid stands for: a page (Pages view) or a
// block (Documents view — a contiguous run of pages from one PDF, as the core
// derives it). Selection, drag and keyboard work on units; every edit is
// still a page run, so the core takes the same move/rotate/remove commands in
// both views and each stays one undo step. Pure.

import type { DocumentBlock } from '@vitela/core';
import { clampSelection, runOf, selectRun, type PageRun, type Selection } from './selection';

export type OrganizeView = 'pages' | 'documents';

export interface Units {
  /** How many cards the grid shows. */
  readonly count: number;
  /** The pages a run of units covers. */
  pageRun(run: PageRun): PageRun;
  /** The page position in front of unit `slot` (`count` is past the last). */
  pageSlot(slot: number): number;
  /** The units that hold a run of pages (the run must lie inside the document). */
  covering(pages: PageRun): PageRun;
}

export function pageUnits(pageCount: number): Units {
  return {
    count: pageCount,
    pageRun: (run) => run,
    pageSlot: (slot) => slot,
    covering: (pages) => pages,
  };
}

export function blockUnits(blocks: readonly DocumentBlock[], pageCount: number): Units {
  const startOf = (slot: number) => blocks[slot]?.start ?? pageCount;
  const blockOf = (page: number) => {
    const index = blocks.findIndex((block) => page < block.start + block.count);
    return index === -1 ? blocks.length - 1 : index;
  };
  return {
    count: blocks.length,
    pageRun: ({ from, count }) => ({ from: startOf(from), count: startOf(from + count) - startOf(from) }),
    pageSlot: startOf,
    covering: ({ from, count }) => {
      const first = blockOf(from);
      return { from: first, count: blockOf(from + count - 1) - first + 1 };
    },
  };
}

export function unitsOf(view: OrganizeView, info: { readonly pageCount: number; readonly blocks: readonly DocumentBlock[] }): Units {
  return view === 'pages' ? pageUnits(info.pageCount) : blockUnits(info.blocks, info.pageCount);
}

/**
 * The same pages as `selection` in `from` units, selected in `to` units (both
 * over one document): the cards of the other view that hold them. Within the
 * same units it is the selection itself, anchor and focus kept.
 */
export function reselect(selection: Selection, from: Units, to: Units): Selection | null {
  const held = clampSelection(selection, from.count);
  if (!held || to.count === 0) return null;
  if (from === to) return held;
  const pages = from.pageRun(runOf(held));
  return pages.count > 0 ? selectRun(to.covering(pages)) : null;
}

/**
 * What a block card is called: the opened PDF's name, the file an import
 * came from (`importedNames`, by source id), or "Blank pages"; with its part
 * number when the PDF is split across blocks.
 */
export function blockLabel(block: DocumentBlock, documentName: string, importedNames: ReadonlyMap<number, string>): string {
  const { source } = block;
  const name =
    source.kind === 'base'
      ? documentName
      : source.kind === 'blank'
        ? 'Blank pages'
        : (importedNames.get(source.id) ?? 'Added PDF');
  return block.part === null ? name : `${name} (part ${block.part})`;
}
