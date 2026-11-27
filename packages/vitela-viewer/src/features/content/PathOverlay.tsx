// What the "Edit content" mode lays over the selected vector path (B26
// T-263): its resize handles — eight on a shape, a corner free unless Shift
// keeps the proportions; two at the ends of a rule, which stays flat — with
// the box it would take drawn while a handle is dragged, and a small toolbar
// with Delete. Presentational: the box a drag ends in is reported, what it
// records is the container's (`usePathEditing`). Moving the path is a drag of
// its body, which the content layer itself reports.

import type { ContentPath, PageRect } from '@vitela/core';
import { toolbarPlace } from './ImageOverlay';
import { pathHandles, resizedPathRect } from './paths';
import { ResizeHandles, useHandleDrag } from './ResizeHandles';

interface PathOverlayProps {
  readonly path: ContentPath;
  /** CSS px per PDF point. */
  readonly scale: number;
  /** Another step is still running, or a cut is being read: nothing can be asked for yet. */
  readonly busy: boolean;
  /** A handle was dragged and let go: the box the path is to take, on the page as drawn. */
  readonly onResize: (rect: PageRect) => void;
  readonly onRemove: () => void;
}

export function PathOverlay({ path, scale, busy, onResize, onRemove }: PathOverlayProps) {
  const drag = useHandleDrag((handle, dx, dy, shift) => resizedPathRect(path.rect, handle, dx, dy, shift), scale, onResize);
  return (
    <>
      <ResizeHandles rect={path.rect} scale={scale} handles={pathHandles(path.rect)} drag={drag} busy={busy} />
      {!drag.preview && (
        <div
          className="image-edit__toolbar"
          role="toolbar"
          aria-label="Shape"
          style={toolbarPlace(path.rect, scale)}
          // The toolbar is not the page: a press on it selects nothing.
          onPointerDown={(event) => event.stopPropagation()}
          onDoubleClick={(event) => event.stopPropagation()}
        >
          <button type="button" aria-label="Delete shape" disabled={busy} onClick={onRemove}>
            Delete
          </button>
        </div>
      )}
    </>
  );
}
