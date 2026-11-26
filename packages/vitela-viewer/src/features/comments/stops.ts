// The stops of Previous/Next annotation, and what names each one. The
// session's annotations have ids; a comment the file carries has none, so it
// is named by its page (the `pageKey`: it follows a moved page, but turning
// the page changes it) and its place among that page's own. Pure.

import type { Annotation, Comment, PageRect, PageSize } from '@vitela/core';
import { annotationBounds, annotationLabel } from '../annotations/editGeometry';

export interface CommentEntry {
  readonly key: string;
  readonly comment: Comment;
}

/** Somewhere Previous/Next stops, on the page as drawn. */
export interface Stop {
  readonly key: string;
  readonly pageIndex: number;
  readonly rect: PageRect;
  /** The session annotation it is; `null` for a comment the file carries. */
  readonly annotationId: number | null;
  /** The session annotation's name; `null` for a comment the file carries. */
  readonly label: string | null;
}

export function sessionKey(id: number): string {
  return `session:${id}`;
}

/** The core's comments, in its order, each with its key. */
export function commentEntries(comments: readonly Comment[], keyOf: (pageIndex: number) => string): CommentEntry[] {
  const seen = new Map<number, number>();
  return comments.map((comment) => {
    if (comment.annotationId !== null) return { key: sessionKey(comment.annotationId), comment };
    const place = seen.get(comment.pageIndex) ?? 0;
    seen.set(comment.pageIndex, place + 1);
    return { key: `file:${keyOf(comment.pageIndex)}:${place}`, comment };
  });
}

/**
 * Every stop, page by page: first the comments the page's file carries, then
 * the session's annotations in paint order — the core's order for comments.
 */
export function stopsOf(annotations: readonly Annotation[], entries: readonly CommentEntry[]): Stop[] {
  const file = entries
    .filter(({ comment }) => comment.annotationId === null)
    .map(({ key, comment }): Stop => ({ key, pageIndex: comment.pageIndex, rect: comment.rect, annotationId: null, label: null }));
  const session = annotations.map(
    (annotation): Stop => ({
      key: sessionKey(annotation.id),
      pageIndex: annotation.pageIndex,
      rect: annotationBounds(annotation),
      annotationId: annotation.id,
      label: annotationLabel(annotation),
    }),
  );
  // A stable sort: each page keeps the file's first, and both keep their own order.
  return [...file, ...session].sort((a, b) => a.pageIndex - b.pageIndex);
}

/** The part of `rect` on its page: a file's `/Rect` may stick out of it. */
export function onPage(rect: PageRect, { widthPt, heightPt }: Pick<PageSize, 'widthPt' | 'heightPt'>): PageRect {
  const hold = (value: number, max: number) => Math.min(Math.max(value, 0), max);
  const left = hold(rect.left, widthPt);
  const top = hold(rect.top, heightPt);
  return {
    left,
    top,
    width: hold(rect.left + rect.width, widthPt) - left,
    height: hold(rect.top + rect.height, heightPt) - top,
  };
}

export function stopLabel(stop: Stop): string {
  return stop.label ?? `Comment on page ${stop.pageIndex + 1}`;
}

/**
 * The stop after (or before) `key`, wrapping at both ends, as the native
 * shells do. From none, or from one that is gone, forward enters at the first
 * and back at the last.
 */
export function stepStop(stops: readonly Stop[], key: string | null, forward: boolean): Stop | null {
  if (stops.length === 0) return null;
  const index = stops.findIndex((stop) => stop.key === key);
  if (index < 0) return (forward ? stops[0] : stops[stops.length - 1])!;
  return stops[(index + (forward ? 1 : stops.length - 1)) % stops.length]!;
}
