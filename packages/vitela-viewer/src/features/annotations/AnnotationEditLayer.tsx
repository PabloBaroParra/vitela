// What makes the annotations of one page selectable and editable: an
// invisible button over each one (a press selects it and drags it; reachable
// with Tab, selected with Enter or Space) and, on the selected one, a handle
// per corner. Sits on top of the text layer but takes the pointer only where
// an annotation is, so a press anywhere else still selects text.
//
// The keys edit only the selected annotation while it (or a handle) has the
// focus: everything else keeps its arrows, so the pages still scroll. What
// is drawn is the annotation layer's, under the text.

import { memo, useRef, type HTMLAttributes, type KeyboardEvent } from 'react';
import type { Annotation, PageRect } from '@vitela/core';
import { moveBy, resizeBy } from './editBuilds';
import {
  annotationBounds,
  annotationLabel,
  canResize,
  cornerPoint,
  CORNERS,
  hitArea,
  sides,
  type Corner,
} from './editGeometry';
import { EDIT_SHORTCUTS, editKey, type EditKey } from './editKeys';
import type { AnnotationDrag } from './useAnnotationDrag';
import type { AnnotationInput } from './useAnnotationEditing';
import { useKeepFocus } from './useKeepFocus';

const CORNER_NAMES: Record<Corner, string> = {
  nw: 'top left',
  ne: 'top right',
  sw: 'bottom left',
  se: 'bottom right',
};

interface AnnotationEditLayerProps {
  /** As shown: the one being dragged where the drag has it. */
  readonly annotations: readonly Annotation[];
  readonly pageKey: string;
  /** CSS px per PDF point at the current zoom. */
  readonly scale: number;
  readonly selectedId: number | null;
  readonly input: AnnotationInput;
  readonly drag: AnnotationDrag;
}

export const AnnotationEditLayer = memo(function AnnotationEditLayer(props: AnnotationEditLayerProps) {
  const { annotations, selectedId, ...shared } = props;
  return (
    <div className="annotation-edit-layer">
      {annotations.map((annotation) => (
        <EditableAnnotation key={annotation.id} {...shared} annotation={annotation} selected={annotation.id === selectedId} />
      ))}
    </div>
  );
});

interface EditableAnnotationProps extends Omit<AnnotationEditLayerProps, 'annotations' | 'selectedId'> {
  readonly annotation: Annotation;
  readonly selected: boolean;
}

function EditableAnnotation({ annotation, selected, pageKey, scale, input, drag }: EditableAnnotationProps) {
  const target = useRef<HTMLDivElement>(null);
  useKeepFocus(target, input.focusLost);
  const resizable = canResize(annotation);

  const run = (key: EditKey, corner: Corner | null) => {
    switch (key.kind) {
      case 'move':
        if (!corner) void input.change(annotation, pageKey, moveBy(key.dx, key.dy));
        else void input.change(annotation, pageKey, cornerMove(corner, key.dx, key.dy));
        return;
      case 'resize':
        if (resizable) void input.change(annotation, pageKey, resizeBy('se', key.dw, key.dh));
        return;
      case 'remove':
        input.remove(annotation, pageKey);
        return;
      case 'deselect':
        target.current?.focus({ preventScroll: true });
        input.deselect();
    }
  };

  const onKeyDown = (event: KeyboardEvent, corner: Corner | null) => {
    if (event.defaultPrevented) return;
    const toggles = !corner && (event.key === 'Enter' || event.key === ' ');
    const key = selected ? editKey(event) : null;
    if (!toggles && !key) return;
    event.preventDefault();
    if (toggles) {
      if (event.repeat) return;
      if (selected) input.deselect();
      else input.select(annotation.id);
    } else if (key?.kind === 'deselect' && drag.cancel()) {
      // Escape gave up the drag; the annotation stays selected.
    } else if (key && !(key.kind === 'remove' && event.repeat)) {
      run(key, corner);
    }
  };

  const pointer = {
    onPointerMove: drag.move,
    onPointerUp: drag.finish,
    onPointerCancel: drag.drop,
    onLostPointerCapture: drag.drop,
  };
  return (
    <>
      <div
        ref={target}
        className="annotation-target"
        role="button"
        tabIndex={0}
        aria-label={annotationLabel(annotation)}
        aria-pressed={selected}
        aria-keyshortcuts={selected ? EDIT_SHORTCUTS : undefined}
        data-target-id={annotation.id}
        style={cssBox(hitArea(annotation), scale)}
        onPointerDown={(event) => {
          if (event.button !== 0 || !event.isPrimary) return;
          input.select(annotation.id);
          drag.start(event, annotation, null);
        }}
        {...pointer}
        // Activated with no pointer (assistive technology).
        onClick={(event) => {
          if (event.detail === 0) input.select(annotation.id);
        }}
        onKeyDown={(event) => onKeyDown(event, null)}
      />
      {selected &&
        resizable &&
        CORNERS.map((corner) => {
          const { x, y } = cornerPoint(annotationBounds(annotation), corner);
          return (
            <Handle
              key={corner}
              corner={corner}
              left={x * scale}
              top={y * scale}
              onPointerDown={(event) => drag.start(event, annotation, corner)}
              {...pointer}
              onKeyDown={(event) => onKeyDown(event, corner)}
              focusLost={input.focusLost}
            />
          );
        })}
    </>
  );
}

interface HandleProps extends HTMLAttributes<HTMLDivElement> {
  readonly corner: Corner;
  readonly left: number;
  readonly top: number;
  readonly focusLost: () => void;
}

function Handle({ corner, left, top, focusLost, ...events }: HandleProps) {
  const handle = useRef<HTMLDivElement>(null);
  useKeepFocus(handle, focusLost);
  return (
    <div
      ref={handle}
      className="annotation-handle"
      role="button"
      tabIndex={0}
      aria-label={`Resize from the ${CORNER_NAMES[corner]}`}
      aria-keyshortcuts={EDIT_SHORTCUTS}
      data-corner={corner}
      style={{ left, top }}
      {...events}
    />
  );
}

/** A corner moved by (`dx`, `dy`): outwards grows the box, inwards shrinks it. */
function cornerMove(corner: Corner, dx: number, dy: number) {
  const { east, south } = sides(corner);
  return resizeBy(corner, east ? dx : -dx, south ? dy : -dy);
}

function cssBox({ left, top, width, height }: PageRect, scale: number) {
  return { left: left * scale, top: top * scale, width: width * scale, height: height * scale };
}
