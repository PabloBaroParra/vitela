// Which pages the Organize grid has selected: always one contiguous run, so
// every operation on it (move, rotate, delete) is one core command and one
// undo step (`movePages`, `rotatePages`, `removePages` take a run). Pure.

/** Where a selection started (`anchor`) and where it was extended to (`focus`, the active card). */
export interface Selection {
  readonly anchor: number;
  readonly focus: number;
}

/** `count` pages starting at `from`, 0-based. */
export interface PageRun {
  readonly from: number;
  readonly count: number;
}

export function selectOnly(index: number): Selection {
  return { anchor: index, focus: index };
}

/** Shift+click / Shift+arrow: the run from the anchor to `index`. */
export function extendTo(selection: Selection, index: number): Selection {
  return { anchor: selection.anchor, focus: index };
}

/** A whole run, with the focus on its last page. */
export function selectRun(run: PageRun): Selection {
  return { anchor: run.from, focus: run.from + run.count - 1 };
}

export function runOf(selection: Selection): PageRun {
  const from = Math.min(selection.anchor, selection.focus);
  return { from, count: Math.abs(selection.anchor - selection.focus) + 1 };
}

export function contains(selection: Selection, index: number): boolean {
  const { from, count } = runOf(selection);
  return index >= from && index < from + count;
}

/** The selection within a document of `pageCount` pages (an undo can take pages away). */
export function clampSelection(selection: Selection | null, pageCount: number): Selection | null {
  if (!selection || pageCount === 0) return null;
  const last = pageCount - 1;
  return { anchor: Math.min(selection.anchor, last), focus: Math.min(selection.focus, last) };
}
