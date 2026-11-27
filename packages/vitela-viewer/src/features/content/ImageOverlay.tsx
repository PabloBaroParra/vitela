// What the "Edit content" mode lays over the selected image (B26 T-260):
// eight resize handles on its box — a corner keeps the proportions, Shift
// frees them, a side changes one dimension — with the box it would take
// drawn while a handle is dragged, and a small toolbar with Replace and
// Delete. Presentational: the box a drag ends in is reported, what it
// records is the container's (`useImageEditing`). Moving the image is a drag
// of its body, which the content layer itself reports.

import type { ContentImage, PageRect } from '@vitela/core';
import { IMAGE_HANDLES, resizedRect } from './images';
import { ResizeHandles, useHandleDrag } from './ResizeHandles';

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

/** How far above the image the toolbar sits, in CSS px. */
const GAP_PX = 8;
/** Its height, to put it under the image when there is no room above. */
const HEIGHT_PX = 34;

/** Where an item's toolbar goes over a box at `rect`: above it, or under it when there is no room above. */
export function toolbarPlace(rect: PageRect, scale: number): { left: number; top: number } {
  const { left, top, height } = rect;
  const above = top * scale - GAP_PX - HEIGHT_PX;
  return { left: Math.max(0, left * scale), top: above >= 0 ? above : (top + height) * scale + GAP_PX };
}

export function ImageOverlay({ image, scale, busy, replaceable, replaceRefusal = null, onResize, onReplace, onRemove }: ImageOverlayProps) {
  // A corner keeps the proportions; Shift frees them.
  const drag = useHandleDrag((handle, dx, dy, shift) => resizedRect(image.rect, handle, dx, dy, shift), scale, onResize);
  const { preview } = drag;
  const toolbarStyle = toolbarPlace(image.rect, scale);
  return (
    <>
      <ResizeHandles rect={image.rect} scale={scale} handles={IMAGE_HANDLES} drag={drag} busy={busy} />
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
