// The selected annotation's toolbar: what it is and where, Move to, Resize,
// Edit text and the colours — only those its kind takes — and Delete. Floats
// over the pages so selecting never moves them under the pointer.
// Presentational.

import type { CSSProperties, RefObject } from 'react';
import type { Annotation, Rgb } from '@vitela/core';
import { PALETTE } from './drawing';
import { formatPt } from './editDialogs';
import { annotationBounds, canRecolor, canResize, canRetext, kindName } from './editGeometry';
import { focusing } from './focusing';
import { colorCss } from './geometry';
import type { DialogKind } from './useAnnotationEditing';
import { useKeepFocus } from './useKeepFocus';

interface AnnotationToolbarProps {
  readonly toolbar: RefObject<HTMLDivElement | null>;
  readonly annotation: Annotation;
  readonly onDialog: (kind: DialogKind) => void;
  readonly onColor: (color: Rgb) => void;
  /** Shows the text of a note. */
  readonly onRead: () => void;
  readonly onRemove: () => void;
  /** The focused control went away with the toolbar. */
  readonly onFocusLost: () => void;
}

export function AnnotationToolbar({ toolbar, annotation, onDialog, onColor, onRead, onRemove, onFocusLost }: AnnotationToolbarProps) {
  useKeepFocus(toolbar, onFocusLost);
  const { left, top, width, height } = annotationBounds(annotation);
  const color = canRecolor(annotation) && 'color' in annotation ? annotation.color : null;
  return (
    <div ref={toolbar} className="annotation-toolbar" role="toolbar" aria-label="Annotation">
      {/* Read out after each keyboard move or resize. */}
      <span className="annotation-toolbar__place" aria-live="polite">
        {`${kindName(annotation)} at ${formatPt(left)}, ${formatPt(top)} pt, ${formatPt(width)} × ${formatPt(height)} pt`}
      </span>
      <button type="button" onClick={focusing(() => onDialog('move'))}>
        Move to…
      </button>
      {canResize(annotation) && (
        <button type="button" onClick={focusing(() => onDialog('resize'))}>
          Resize…
        </button>
      )}
      {annotation.kind === 'note' && (
        <button type="button" onClick={focusing(onRead)}>
          Read note…
        </button>
      )}
      {canRetext(annotation) && (
        <button type="button" onClick={focusing(() => onDialog('text'))}>
          Edit text…
        </button>
      )}
      {color && (
        <span className="draw-toolbar__colors" role="group" aria-label="Annotation colour">
          {PALETTE.map(({ name, color: swatch }) => (
            <button
              key={name}
              type="button"
              className="draw-color"
              aria-label={name}
              aria-pressed={color.r === swatch.r && color.g === swatch.g && color.b === swatch.b}
              style={{ '--swatch': colorCss(swatch) } as CSSProperties}
              onClick={() => onColor(swatch)}
            />
          ))}
        </span>
      )}
      <button type="button" aria-keyshortcuts="Delete Backspace" onClick={focusing(onRemove)}>
        Delete
      </button>
    </div>
  );
}
