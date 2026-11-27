// The empty box Insert text opens where the page was pressed (B26 T-261): a
// one-line field hanging from the press in the font and size the core will
// paint the line in (14 pt Helvetica), turned with the page about that
// corner, and outlined — unlike a retype there is no text under it to show
// where the line lands. Enter commits, Escape cancels, leaving it commits.
// Presentational: the draft and what a commit records are the container's
// (`useTextInsertion`).

import { useEffect, useRef, type CSSProperties } from 'react';
import { INSERT_TEXT_SIZE_PT } from './textBlocks';
import type { InsertEditor } from './useTextInsertion';

interface TextInsertEditorProps {
  readonly editor: InsertEditor;
  /** CSS px per PDF point. */
  readonly scale: number;
  readonly rotationDegrees: number;
  /** Enter (`refocus`: give the keyboard back to the pages) or leaving the field. */
  readonly onCommit: (refocus: boolean) => void;
  readonly onCancel: () => void;
  readonly onType: (text: string) => void;
  /** The field is mounted, and when it goes (its page slot unmounted, or the editor closed). */
  readonly onAttach: () => void;
  readonly onDetach: () => void;
}

/** About how wide a glyph is for its size: the field grows with what is typed. */
const AVERAGE_ADVANCE = 0.6;
/** The field's width before anything is typed, in ems. */
const MIN_WIDTH_EM = 8;

export function TextInsertEditor({ editor, scale, rotationDegrees, onCommit, onCancel, onType, onAttach, onDetach }: TextInsertEditorProps) {
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    field.current?.focus({ preventScroll: true });
    onAttach();
    return onDetach;
    // Once per mount: the callbacks are the container's, and stable.
  }, []);

  const fontPx = INSERT_TEXT_SIZE_PT * scale;
  const css: CSSProperties = {
    left: editor.at.x * scale,
    top: editor.at.y * scale,
    width: Math.max(MIN_WIDTH_EM, editor.draft.length * AVERAGE_ADVANCE + 1) * fontPx,
    height: fontPx,
    fontSize: fontPx,
    fontFamily: 'Helvetica, Arial, sans-serif',
    transform: rotationDegrees ? `rotate(${rotationDegrees}deg)` : undefined,
  };
  return (
    <input
      ref={field}
      className="text-insert-editor"
      aria-label="New text"
      value={editor.draft}
      spellCheck={false}
      autoComplete="off"
      style={css}
      onChange={(event) => onType(event.target.value)}
      // The pages' keys (arrows, Delete, Escape that disarms) are not this field's.
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === 'Enter') {
          event.preventDefault();
          onCommit(true);
        } else if (event.key === 'Escape') {
          event.preventDefault();
          onCancel();
        }
      }}
      onBlur={() => onCommit(false)}
      onPointerDown={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
    />
  );
}
