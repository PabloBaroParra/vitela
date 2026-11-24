// The selected annotation's dialogs: Move to, Resize and Edit text. Each
// turns what was typed into the edit to make (or says in the dialog why it
// cannot be), in points from the top-left corner of the page as drawn.

import { useState } from 'react';
import type { Annotation, Engine, PageSize } from '@vitela/core';
import { moveTo, resizeTo, retext, type EditBuild } from './editBuilds';
import { formatPt, parseMove, parseResize, proportional } from './editDialogs';
import { annotationBounds, keepsProportions, resizeFloor } from './editGeometry';
import { FreeTextDialog } from './FreeTextDialog';
import { freeTextFrame } from './geometry';
import { LengthsDialog } from './LengthsDialog';
import type { AnnotationDialogState } from './useAnnotationEditing';

interface AnnotationDialogProps {
  readonly engine: Engine;
  readonly dialog: AnnotationDialogState;
  readonly annotation: Annotation;
  /** The annotation's page as drawn. */
  readonly page: PageSize;
  readonly onChange: () => void;
  readonly onSubmit: (build: EditBuild) => void;
  readonly onCancel: () => void;
}

export function AnnotationDialog(props: AnnotationDialogProps) {
  switch (props.dialog.kind) {
    case 'move':
      return <MoveDialog {...props} />;
    case 'resize':
      return <ResizeDialog {...props} />;
    case 'text':
      return <RetextDialog {...props} />;
  }
}

function MoveDialog({ dialog, annotation, page, onSubmit, onCancel }: AnnotationDialogProps) {
  const bounds = annotationBounds(annotation);
  const [left, setLeft] = useState(formatPt(bounds.left));
  const [top, setTop] = useState(formatPt(bounds.top));
  const [error, setError] = useState<string | null>(null);
  const typed = (set: (value: string) => void) => (value: string) => {
    set(value);
    setError(null);
  };
  return (
    <LengthsDialog
      title="Move to"
      submit="Move"
      fields={[
        { label: 'Left', value: left, onChange: typed(setLeft) },
        { label: 'Top', value: top, onChange: typed(setTop) },
      ]}
      busy={dialog.busy}
      error={error}
      onSubmit={() => {
        const answer = parseMove(left, top, bounds, page);
        if ('error' in answer) setError(answer.error);
        else onSubmit(moveTo(answer.left, answer.top));
      }}
      onCancel={onCancel}
    />
  );
}

function ResizeDialog({ dialog, annotation, page, onSubmit, onCancel }: AnnotationDialogProps) {
  const rect = annotationBounds(annotation);
  const [width, setWidth] = useState(formatPt(rect.width));
  const [height, setHeight] = useState(formatPt(rect.height));
  const [keep, setKeep] = useState(keepsProportions(annotation));
  const [error, setError] = useState<string | null>(null);
  const typedWidth = (value: string) => {
    setWidth(value);
    setError(null);
    const other = keep ? proportional(value, 'width', rect) : null;
    if (other !== null) setHeight(other);
  };
  const typedHeight = (value: string) => {
    setHeight(value);
    setError(null);
    const other = keep ? proportional(value, 'height', rect) : null;
    if (other !== null) setWidth(other);
  };
  return (
    <LengthsDialog
      title="Resize"
      submit="Resize"
      fields={[
        { label: 'Width', value: width, onChange: typedWidth },
        { label: 'Height', value: height, onChange: typedHeight },
      ]}
      busy={dialog.busy}
      error={error}
      onSubmit={() => {
        const answer = parseResize(width, height, rect, resizeFloor(annotation, page.rotationDegrees), page);
        if ('error' in answer) setError(answer.error);
        else onSubmit(resizeTo(answer.width, answer.height));
      }}
      onCancel={onCancel}
    >
      <label className="lengths-dialog__keep">
        <input
          type="checkbox"
          checked={keep}
          onChange={(event) => {
            setKeep(event.target.checked);
            // Kept from here on: the height follows the width now typed.
            const other = event.target.checked ? proportional(width, 'width', rect) : null;
            if (other !== null) setHeight(other);
          }}
        />
        Keep proportions
      </label>
    </LengthsDialog>
  );
}

function RetextDialog({ engine, dialog, annotation, page, onChange, onSubmit, onCancel }: AnnotationDialogProps) {
  const { width, height } = freeTextFrame(annotationBounds(annotation), page.rotationDegrees);
  return (
    <FreeTextDialog
      engine={engine}
      title="Edit the text box"
      submit="Save text"
      initialText={'contents' in annotation ? annotation.contents : ''}
      frame={{ width, height }}
      busy={dialog.busy}
      error={dialog.error}
      onChange={onChange}
      onSubmit={(text) => onSubmit(retext(text))}
      onCancel={onCancel}
    />
  );
}
