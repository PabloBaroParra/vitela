// The camouflaged editor over a text run (B26 T-259): a one-line field laid
// exactly over the run, turned with the page, in the run's own font (its
// embedded program when the browser loaded it, else the family its name
// suggests), size, colour, weight and slant, so typing looks like editing
// the page. Enter commits, Escape cancels, leaving it commits.
// Presentational: what is typed (the draft), whether a commit counts and what
// it records are the container's (`useTextRunEditing`), so the draft outlives
// the field when its page slot is unmounted by a scroll (`onDetach`).

import { useEffect, useRef, type CSSProperties } from 'react';
import type { TextRunStyle } from '@vitela/core';
import { editorBox, faceOf, fallbackFamilies } from './textRuns';
import type { OpenEditor } from './useTextRunEditing';

interface TextRunEditorProps {
  readonly editor: OpenEditor;
  /** CSS px per PDF point. */
  readonly scale: number;
  readonly rotationDegrees: number;
  /** Enter (`refocus`: give the keyboard back to the pages) or leaving the field. */
  readonly onCommit: (refocus: boolean) => void;
  readonly onCancel: () => void;
  /** Each change of the text: the draft is the container's. */
  readonly onType: (text: string) => void;
  /** The field is mounted, and when it goes (its page slot unmounted, or the editor closed). */
  readonly onAttach: () => void;
  readonly onDetach: () => void;
}

/** About how wide a glyph is for its size: the field grows with what is typed. */
const AVERAGE_ADVANCE = 0.6;

export function TextRunEditor({ editor, scale, rotationDegrees, onCommit, onCancel, onType, onAttach, onDetach }: TextRunEditorProps) {
  const { run, style, look, family, draft: text } = editor;
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    field.current?.focus({ preventScroll: true });
    field.current?.select();
    onAttach();
    return onDetach;
    // Once per mount: the callbacks are the container's, and stable.
  }, []);

  const box = editorBox(run.rect, rotationDegrees, scale);
  const fontPx = (style?.fontSizePt ?? run.rect.height * 0.8) * scale;
  const width = Math.max(box.width, text.length * fontPx * AVERAGE_ADVANCE);
  const face = faceOf(style, look);
  const families = [...(family ? [family] : []), ...fallbackFamilies(style?.baseFont ?? '')];
  const css: CSSProperties = {
    left: box.left,
    top: box.top,
    width,
    height: box.height,
    transform: box.rotation ? `rotate(${box.rotation}deg)` : undefined,
    fontFamily: families.map(quoteFamily).join(', '),
    fontSize: fontPx,
    fontWeight: face.bold ? 700 : 400,
    fontStyle: face.italic ? 'italic' : 'normal',
    letterSpacing: style ? style.charSpacingPt * scale : undefined,
    color: colorOf(style),
  };
  return (
    <input
      ref={field}
      className="text-run-editor"
      aria-label={`Edit text “${run.text.trim()}”`}
      value={text}
      spellCheck={false}
      autoComplete="off"
      style={css}
      data-family={family ?? ''}
      onChange={(event) => onType(event.target.value)}
      // The pages' keys (arrows move the run, Delete removes it) are not this field's.
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

function quoteFamily(family: string): string {
  return ['serif', 'sans-serif', 'monospace'].includes(family) ? family : `"${family.replace(/"/g, '')}"`;
}

function colorOf(style: TextRunStyle | null): string {
  if (!style) return 'rgb(0 0 0)';
  const { r, g, b } = style.color;
  return `rgb(${r} ${g} ${b})`;
}
