// Turns "put the selected pages there" into the core's move command. Only the
// arithmetic of positions lives here; whether the move is allowed, and the
// move itself, are the core's (`EditCommand.movePages`). Pure.

import type { PageEdit } from '@vitela/core';
import type { PageRun } from './selection';
import type { Units } from './units';

export type MoveEdit = Extract<PageEdit, { kind: 'move' }>;

/**
 * The move that puts `run` in front of the page now at `slot` (`0..pageCount`;
 * `pageCount` is past the last page), or `null` when that leaves it where it
 * is. The core's `to` is where the run's first page ends up once the run is
 * out of the way, so a slot after the run counts the run out.
 */
export function moveToSlot(run: PageRun, slot: number): MoveEdit | null {
  if (slot >= run.from && slot <= run.from + run.count) return null;
  const to = slot < run.from ? slot : slot - run.count;
  return { kind: 'move', from: run.from, count: run.count, to };
}

/** The move of `run` by `delta` places, stopping at either end; `null` when it cannot move. */
export function moveBy(run: PageRun, delta: number, pageCount: number): MoveEdit | null {
  const to = Math.max(0, Math.min(run.from + delta, pageCount - run.count));
  return to === run.from ? null : { kind: 'move', from: run.from, count: run.count, to };
}

/** Where the moved pages are once the move is applied. */
export function runAfterMove(move: MoveEdit): PageRun {
  return { from: move.to, count: move.count };
}

/**
 * The move that puts the units of `run` in front of unit `slot`
 * (`0..units.count`), as the page move the core takes; `null` when that
 * leaves them where they are.
 */
export function moveUnitsToSlot(units: Units, run: PageRun, slot: number): MoveEdit | null {
  return moveToSlot(units.pageRun(run), units.pageSlot(slot));
}

/**
 * The move of the units of `run` by `delta` units, stopping at either end:
 * past `|delta|` units, whatever their page counts. `null` when it cannot move.
 */
export function moveUnitsBy(units: Units, run: PageRun, delta: number): MoveEdit | null {
  const slot = delta < 0 ? run.from + delta : run.from + run.count + delta;
  return moveUnitsToSlot(units, run, Math.max(0, Math.min(units.count, slot)));
}
