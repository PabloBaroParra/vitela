// A job drawn from one of the engine's page snapshots (what a save would
// write now): Export images and Print. Both wait for the queued edits, open
// the snapshot, draw its pages one at a time and close it however the job
// ends; this module is that shared lifecycle, and each feature keeps what it
// does with a drawn page (a file out, a sheet in the print frame) and what it
// says. Pure use-case orchestration over the `Engine`; no React, no markup.

import type { Engine, ImageExport, ImageExportFormat } from '@vitela/core';

/** Whether a running job still matters, and whether it was asked to stop. */
export interface JobGuard {
  /** False once the editor is gone or shows another document: the job is dropped, silently. */
  readonly current: () => boolean;
  /** True once the user asked the job to stop. */
  readonly stopped: () => boolean;
}

/**
 * Waits for the queued edits (`settle`), opens a snapshot with `open`, runs
 * `job` on it and closes it however `job` ends — the snapshot is the
 * engine's memory. `undefined`, with `job` never run, when the job was
 * dropped before the snapshot was in hand. `job`'s failure, or `open`'s,
 * reaches the caller.
 */
export async function withSnapshot<T>(
  engine: Engine,
  guard: JobGuard,
  settle: (() => Promise<unknown>) | undefined,
  open: () => Promise<ImageExport>,
  job: (snapshot: ImageExport) => Promise<T>,
): Promise<T | undefined> {
  await settle?.();
  if (!guard.current()) return undefined;
  const snapshot = await open();
  try {
    if (!guard.current()) return undefined;
    return await job(snapshot);
  } finally {
    await engine.closeImageExport(snapshot.handle).catch(() => false);
  }
}

/** How drawing ended: every page handed over, stopped by the user, or dropped. */
export type DrawEnd = 'finished' | 'stopped' | 'dropped';

/** The engine could not draw the page at `pageIndex` (zero-based); `cause` is its error. */
export class PageNotDrawn extends Error {
  constructor(
    readonly pageIndex: number,
    override readonly cause: unknown,
  ) {
    super(`page ${pageIndex + 1} was not drawn`);
  }
}

/** What a job does around each page. */
export interface PageSteps {
  /** Before the page at `pageIndex` is drawn; `index` counts `pages` from 0. */
  readonly starting: (pageIndex: number, index: number) => void;
  /** Takes the page's encoded bytes. A failure here ends the drawing with it, as it is. */
  readonly drawn: (bytes: Uint8Array, pageIndex: number, index: number) => void | Promise<void>;
}

/**
 * Draws `pages` of the open snapshot one at a time, at `dpi` as `format`,
 * handing each to `steps.drawn` before the next is drawn. A stop is heard
 * before each page (the page in hand still goes out); a dropped job hands
 * over nothing more. A page the engine cannot draw throws `PageNotDrawn`.
 */
export async function drawPages(
  engine: Engine,
  snapshot: number,
  pages: readonly number[],
  dpi: number,
  format: ImageExportFormat,
  guard: JobGuard,
  steps: PageSteps,
): Promise<DrawEnd> {
  for (const [index, pageIndex] of pages.entries()) {
    if (!guard.current()) return 'dropped';
    if (guard.stopped()) return 'stopped';
    steps.starting(pageIndex, index);
    let bytes: Uint8Array;
    try {
      bytes = await engine.exportPageImage(snapshot, pageIndex, dpi, format);
    } catch (error) {
      throw new PageNotDrawn(pageIndex, error);
    }
    if (!guard.current()) return 'dropped';
    await steps.drawn(bytes, pageIndex, index);
  }
  return 'finished';
}
