// Image stamps in the viewer: the image picked for the Image tool, and
// stamping an image — the armed one where the page is clicked, or a file
// dropped on a page where it was dropped. Container logic only.
//
// The core does all the placing: `stampPlacement` turns the anchor into the
// stamp's box (longest side two inches, the image's top-left corner at the
// anchor, turned with the page) and `addStamp` adds it through the viewer's
// queue (`useAnnotationEdits.add`), one undo step, refused like any drawn
// annotation if the page under it changed. The engine never hands the image
// back, so the caller keeps it (`keep`) by the id the add reported — in the
// same step, before the layer draws the stamp. A redo brings the stamp back
// with that same id, and ids are never reused, so what is kept stays right.

import { useCallback, useState } from 'react';
import { errorMessage } from '@vitela/core';
import type { DocumentHandle, Engine, PagePoint } from '@vitela/core';
import { dataUrl, imageType, stampEdit } from './stamps';
import { NOT_DRAWN, type DrawingEdits } from './useDrawing';

/** An image ready to stamp: its bytes for the core, a URL to draw it. */
export interface StampImage {
  readonly name: string;
  readonly bytes: Uint8Array;
  readonly href: string;
}

export const NOT_A_STAMP_IMAGE = 'Only PNG and JPEG images can be added to a page.';
export const STAMP_NOT_ALLOWED = 'This PDF does not allow annotations, so the image was not added.';

/** `file`'s bytes as a stamp image, or `null` when they are not a PNG or JPEG. */
async function readStampImage(file: File): Promise<StampImage | null> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const type = imageType(bytes);
  return type && { name: file.name, bytes, href: dataUrl(bytes, type) };
}

export function useStamps(
  engine: Engine,
  handle: DocumentHandle,
  edits: DrawingEdits,
  keep: (id: number, href: string) => void,
) {
  const [image, setImage] = useState<StampImage | null>(null);
  const { allowed, add, notify } = edits;

  const read = useCallback(
    async (file: File) => {
      const read = await readStampImage(file).catch(() => null);
      if (!read) notify(NOT_A_STAMP_IMAGE);
      return read;
    },
    [notify],
  );

  const stamp = useCallback(
    async (stamped: StampImage, pageIndex: number, pageKey: string, anchor: PagePoint) => {
      if (!allowed) {
        notify(STAMP_NOT_ALLOWED);
        return;
      }
      let rect;
      try {
        rect = await engine.stampPlacement(handle, pageIndex, stamped.bytes, anchor);
      } catch (error) {
        // `InvalidImage`: the bytes looked right but do not decode.
        notify(errorMessage(error));
        return;
      }
      const outcome = await add(stampEdit(pageIndex, rect, stamped.bytes), pageIndex, pageKey, [], (id) =>
        keep(id, stamped.href),
      );
      // A failure is the viewer's to show already; a refusal leaves nothing.
      if (outcome.status === 'refused') notify(NOT_DRAWN);
    },
    [add, allowed, engine, handle, keep, notify],
  );

  /** Reads `file` as the image the Image tool stamps; `false` (and why) when it is not one. */
  const pick = useCallback(
    async (file: File) => {
      const picked = await read(file);
      if (picked) setImage(picked);
      return picked !== null;
    },
    [read],
  );

  /** Stamps the picked image at `anchor` on page `pageIndex`, which held `pageKey` at the click. */
  const place = useCallback(
    async (pageIndex: number, pageKey: string, anchor: PagePoint) => {
      if (image) await stamp(image, pageIndex, pageKey, anchor);
    },
    [image, stamp],
  );

  /** Stamps `file`, dropped at `anchor` on page `pageIndex`; the Image tool keeps its own image. */
  const drop = useCallback(
    async (pageIndex: number, pageKey: string, anchor: PagePoint, file: File) => {
      const dropped = await read(file);
      if (dropped) await stamp(dropped, pageIndex, pageKey, anchor);
    },
    [read, stamp],
  );

  return { image, pick, place, drop };
}
