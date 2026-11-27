// Image stamps in the viewer: the image picked for the Image tool, and
// stamping an image — the armed one where the page is clicked, or a file
// dropped on a page where it was dropped. Container logic only; returns
// handlers the viewer hands straight to the drawing tools and the pages.
//
// The core does all the placing: `stampPlacement` turns the anchor into the
// stamp's box (longest side two inches, the image's top-left corner at the
// anchor, turned with the page) and `addStamp` adds it. Both run inside one
// step of the viewer's queue (`useAnnotationEdits.add` builds the edit in
// the step), so the page is busy from the click on: a second click or drop
// meanwhile is refused before it asks the core anything, as a second stroke
// is, and the step is refused like any drawn annotation if the page under it
// changed. The engine never hands the image back, so the caller keeps it
// (`keep`) by the id the add reported — in the same step, before the layer
// draws the stamp. A redo brings the stamp back with that same id, and ids
// are never reused, so what is kept stays right.
//
// A file is checked before the core sees it: over `MAX_STAMP_BYTES` it is
// not even read; its bytes must then read and start like a PNG or JPEG.
// Picks may finish reading out of order: only the latest one is kept.

import { useCallback, useRef, useState } from 'react';
import type { DocumentHandle, Engine, PagePoint } from '@vitela/core';
import { dataUrl, imageType, MAX_STAMP_BYTES, stampEdit } from './stamps';
import { NOT_DRAWN, type DrawingEdits } from './useDrawing';

/** An image ready to stamp: its bytes for the core, a URL to draw it. */
export interface StampImage {
  readonly name: string;
  readonly bytes: Uint8Array;
  readonly href: string;
}

/** Stamps the armed image at `anchor` on page `pageIndex`, which held `pageKey` at the click. */
export type PlaceStamp = (pageIndex: number, pageKey: string, anchor: PagePoint) => void;

/** Stamps `file`, dropped at `anchor` (drawn points, on the page) on page `pageIndex`, which held `pageKey`. */
export type ImageDrop = (pageIndex: number, pageKey: string, anchor: PagePoint, file: File) => void;

/**
 * Reads `file` as the Image tool's image: `true` once it is the armed image,
 * `false` when it is not one (and the viewer says why) or a later pick
 * replaced it while it was being read.
 */
export type PickStamp = (file: File) => Promise<boolean>;

export const NOT_A_STAMP_IMAGE = 'Only PNG and JPEG images can be added to a page.';
export const STAMP_NOT_READ = 'The image could not be read. Nothing was added.';
export const STAMP_TOO_LARGE = `Images larger than ${MAX_STAMP_BYTES / (1024 * 1024)} MB cannot be added to a page.`;
export const STAMP_NOT_ALLOWED = 'This PDF does not allow annotations, so the image was not added.';

export type ReadResult = { readonly image: StampImage } | { readonly refusal: string };

/** `file` as a stamp image, or why it cannot be one. Never throws. */
export async function readStampImage(file: File): Promise<ReadResult> {
  if (file.size > MAX_STAMP_BYTES) return { refusal: STAMP_TOO_LARGE };
  let bytes: Uint8Array;
  try {
    bytes = new Uint8Array(await file.arrayBuffer());
  } catch {
    return { refusal: STAMP_NOT_READ };
  }
  const type = imageType(bytes);
  if (!type) return { refusal: NOT_A_STAMP_IMAGE };
  return { image: { name: file.name, bytes, href: dataUrl(bytes, type) } };
}

export function useStamps(
  engine: Engine,
  handle: DocumentHandle,
  edits: DrawingEdits,
  keep: (id: number, href: string) => void,
) {
  const [image, setImage] = useState<StampImage | null>(null);
  /** Counts picks: a read that finishes after a later pick began is dropped. */
  const latestPick = useRef(0);
  const { allowed, add, notify } = edits;

  /** `file` as a dropped stamp image, or `null` once the viewer says why not. */
  const readOrNotify = useCallback(
    async (file: File) => {
      const result = await readStampImage(file);
      if ('refusal' in result) {
        notify(result.refusal);
        return null;
      }
      return result.image;
    },
    [notify],
  );

  const stamp = useCallback(
    async (stamped: StampImage, pageIndex: number, pageKey: string, anchor: PagePoint) => {
      if (!allowed) {
        notify(STAMP_NOT_ALLOWED);
        return;
      }
      // Placed inside the step: an `InvalidImage` (the bytes looked right but
      // do not decode) fails it, and the viewer shows the core's message.
      const placed = async () => {
        const rect = await engine.stampPlacement(handle, pageIndex, stamped.bytes, anchor);
        return stampEdit(pageIndex, rect, stamped.bytes);
      };
      const outcome = await add(placed, pageIndex, pageKey, [], (id) => keep(id, stamped.href));
      // A failure is the viewer's to show already; a refusal leaves nothing.
      if (outcome.status === 'refused') notify(NOT_DRAWN);
    },
    [add, allowed, engine, handle, keep, notify],
  );

  const pick = useCallback<PickStamp>(
    async (file) => {
      const ticket = ++latestPick.current;
      const result = await readStampImage(file);
      // Replaced while it was read: neither it nor why it failed matters now.
      if (ticket !== latestPick.current) return false;
      if ('refusal' in result) {
        notify(result.refusal);
        return false;
      }
      setImage(result.image);
      return true;
    },
    [notify],
  );

  const place = useCallback<PlaceStamp>(
    (pageIndex, pageKey, anchor) => {
      if (image) void stamp(image, pageIndex, pageKey, anchor);
    },
    [image, stamp],
  );

  // The Image tool keeps its own image: a drop arms nothing.
  const drop = useCallback<ImageDrop>(
    (pageIndex, pageKey, anchor, file) => {
      void readOrNotify(file).then((dropped) => dropped && stamp(dropped, pageIndex, pageKey, anchor));
    },
    [readOrNotify, stamp],
  );

  return { image, pick, place, drop };
}
