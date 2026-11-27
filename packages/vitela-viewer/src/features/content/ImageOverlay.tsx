// What the "Edit content" mode lays over the selected image (B26 T-260):
// eight resize handles on its box — a corner keeps the proportions, Shift
// frees them, a side changes one dimension — with the box it would take
// drawn while a handle is dragged, and a small toolbar with Replace and
// Delete. Presentational: the box a drag ends in is reported, what it
// records is the container's (`useImageEditing`). Moving the image is a drag
// of its body, which the content layer itself reports.

import type { ContentImage, PageRect } from '@vitela/core';
import { useState } from 'react';
import { handleBox, IMAGE_HANDLES, resizedRect, type ImageHandle } from './images';
import { isDrag } from './textRuns';

interface ImageOverlayProps {
  readonly image: ContentImage;
  /** CSS px per PDF point. */
  readonly scale: number;
  /** Another step is still running: nothing can be asked for yet. */
  readonly busy: boolean;
  /** Whether the core said the image can be replaced (`false` while it is asked, or refused). */
  readonly replaceable: boolean;
  /** Why it cannot, as the core's sentence; `null` when it can or is still being asked. */
  readonly replaceRefusal?: string | null;
  /** A handle was dragged and let go: the box the image is to take, on the page as drawn. */
  readonly onResize: (rect: PageRect) => void;
  readonly onReplace: () => void;
  readonly onRemove: () => void;
}

const HANDLE_LABELS: Record<ImageHandle, string> = {
  nw: 'top-left corner',
  n: 'top side',
  ne: 'top-right corner',
  e: 'right side',
  se: 'bottom-right corner',
  s: 'bottom side',
  sw: 'bottom-left corner',
  w: 'left side',
};

/** How far above the image the toolbar sits, in CSS px. */
const GAP_PX = 8;
/** Its height, to put it under the image when there is no room above. */
const HEIGHT_PX = 34;

/** A handle being dragged: where its press started, in client px. */
interface Grab {
  readonly handle: ImageHandle;
  readonly pointerId: number;
  readonly x: number;
  readonly y: number;
}

export function ImageOverlay({ image, scale, busy, replaceable, replaceRefusal = null, onResize, onReplace, onRemove }: ImageOverlayProps) {
  const [grab, setGrab] = useState<Grab | null>(null);
  const [preview, setPreview] = useState<PageRect | null>(null);
  const boxOf = (held: Grab, clientX: number, clientY: number, free: boolean) =>
    resizedRect(image.rect, held.handle, (clientX - held.x) / scale, (clientY - held.y) / scale, free);
  const { left, top, height } = image.rect;
  const above = top * scale - GAP_PX - HEIGHT_PX;
  const toolbarStyle = { left: Math.max(0, left * scale), top: above >= 0 ? above : (top + height) * scale + GAP_PX };
  return (
    <>
      {preview && (
        <div
          className="image-edit__preview"
          style={{ left: preview.left * scale, top: preview.top * scale, width: preview.width * scale, height: preview.height * scale }}
        />
      )}
      {!busy &&
        IMAGE_HANDLES.map((handle) => (
          <button
            key={handle}
            type="button"
            // Pointer-only: the keyboard moves the image with the arrows.
            tabIndex={-1}
            className={`image-edit__handle image-edit__handle--${handle}`}
            aria-label={`Resize from the ${HANDLE_LABELS[handle]}`}
            style={handleBox(image.rect, handle, scale)}
            onPointerDown={(event) => {
              // The handle is not the page: the press neither selects nor drags the image.
              event.stopPropagation();
              event.preventDefault();
              if (event.button !== 0 || !event.isPrimary) return;
              event.currentTarget.setPointerCapture?.(event.pointerId);
              setGrab({ handle, pointerId: event.pointerId, x: event.clientX, y: event.clientY });
            }}
            onPointerMove={(event) => {
              event.stopPropagation();
              if (!grab || grab.pointerId !== event.pointerId) return;
              if (preview || isDrag(event.clientX - grab.x, event.clientY - grab.y)) {
                setPreview(boxOf(grab, event.clientX, event.clientY, event.shiftKey));
              }
            }}
            onPointerUp={(event) => {
              event.stopPropagation();
              if (!grab || grab.pointerId !== event.pointerId) return;
              setGrab(null);
              setPreview(null);
              if (preview || isDrag(event.clientX - grab.x, event.clientY - grab.y)) {
                onResize(boxOf(grab, event.clientX, event.clientY, event.shiftKey));
              }
            }}
            onPointerCancel={() => {
              setGrab(null);
              setPreview(null);
            }}
            onDoubleClick={(event) => event.stopPropagation()}
          />
        ))}
      {!preview && (
        <div
          className="image-edit__toolbar"
          role="toolbar"
          aria-label="Image"
          style={toolbarStyle}
          // The toolbar is not the page: a press on it selects nothing.
          onPointerDown={(event) => event.stopPropagation()}
          onDoubleClick={(event) => event.stopPropagation()}
        >
          <button
            type="button"
            aria-label="Replace image"
            aria-disabled={busy || !replaceable ? 'true' : undefined}
            title={replaceRefusal ?? undefined}
            onClick={() => {
              if (!busy) onReplace();
            }}
          >
            Replace
          </button>
          <button type="button" aria-label="Delete image" disabled={busy} onClick={onRemove}>
            Delete
          </button>
        </div>
      )}
    </>
  );
}
