// The small toolbar by the selected text run (B26 T-259): Edit text, Edit
// paragraph when the run is part of one (T-261), the bold / italic / underline toggles (pressed when the run looks that way,
// a queued restyle included) and Delete. A style the core cannot take off
// (a bold font whose page has no regular sibling) is not offered off.
// Presentational.

import type { ContentRun, TextRunLookState } from '@vitela/core';
import type { FaceToggle } from './textRuns';

interface TextRunToolbarProps {
  readonly run: ContentRun;
  /** CSS px per PDF point. */
  readonly scale: number;
  /** How the run looks now; `null` while it is read, or when the file cannot say. */
  readonly look: TextRunLookState | null;
  /** Another step is still running: nothing can be asked for yet. */
  readonly busy: boolean;
  readonly onEdit: () => void;
  /** Opens the editor over the run's whole paragraph; absent for a run on one line. */
  readonly onEditParagraph?: () => void;
  readonly onToggle: (face: FaceToggle) => void;
  readonly onRemove: () => void;
}

/** How far above the run the toolbar sits, in CSS px. */
const GAP_PX = 6;
/** Its height, to put it under the run when there is no room above. */
const HEIGHT_PX = 34;

const FACES: readonly { readonly face: FaceToggle; readonly label: string; readonly glyph: string }[] = [
  { face: 'bold', label: 'Bold', glyph: 'B' },
  { face: 'italic', label: 'Italic', glyph: 'I' },
  { face: 'underline', label: 'Underline', glyph: 'U' },
];

export function TextRunToolbar({ run, scale, look, busy, onEdit, onEditParagraph, onToggle, onRemove }: TextRunToolbarProps) {
  const { left, top, height } = run.rect;
  const above = top * scale - GAP_PX - HEIGHT_PX;
  const style = { left: Math.max(0, left * scale), top: above >= 0 ? above : (top + height) * scale + GAP_PX };
  const removable = (face: FaceToggle) =>
    !look || !look[face] || (face === 'bold' ? look.canRemoveBold : face === 'italic' ? look.canRemoveItalic : true);
  return (
    <div
      className="text-run-toolbar"
      role="toolbar"
      aria-label="Text run"
      style={style}
      // The toolbar is not the page: a press on it selects nothing.
      onPointerDown={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
    >
      <button type="button" disabled={busy || !run.editable} onClick={onEdit}>
        Edit text
      </button>
      {onEditParagraph && (
        <button type="button" disabled={busy || !run.editable} onClick={onEditParagraph}>
          Edit paragraph
        </button>
      )}
      {FACES.map(({ face, label, glyph }) => (
        <button
          key={face}
          type="button"
          className={`text-run-toolbar__face text-run-toolbar__face--${face}`}
          aria-label={label}
          title={label}
          aria-pressed={look ? look[face] : false}
          disabled={busy || !look || !removable(face)}
          onClick={() => onToggle(face)}
        >
          {glyph}
        </button>
      ))}
      <button type="button" disabled={busy} onClick={onRemove}>
        Delete
      </button>
    </div>
  );
}
