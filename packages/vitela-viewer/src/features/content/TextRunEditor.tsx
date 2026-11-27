// The camouflaged editor over a text run (B26 T-259): a one-line field laid
// exactly over the run, turned with the page, in the run's own font (its
// embedded program when the browser loaded it, else the family its name
// suggests), size, colour, weight and slant, so typing looks like editing
// the page. Enter commits, Escape cancels, leaving it commits.
// Presentational: what a commit records is the container's.

import { useEffect, useRef, useState, type CSSProperties } from 'react';
import type { TextRunStyle } from '@vitela/core';
import { editorBox, faceOf, fallbackFamilies } from './textRuns';
import type { OpenEditor } from './useTextRunEditing';

interface TextRunEditorProps {
  readonly editor: OpenEditor;
  /** CSS px per PDF point. */
  readonly scale: number;
  readonly rotationDegrees: number;
  /** Enter (`refocus`: give the keyboard back to the pages) or leaving the field. */
  readonly onCommit: (text: string, refocus: boolean) => void;
  readonly onCancel: () => void;
  /** Each change of the text, so a save or leaving the mode commits what is typed. */
  readonly onType: (text: string) => void;
}

/** About how wide a glyph is for its size: the field grows with what is typed. */
const AVERAGE_ADVANCE = 0.6;

export function TextRunEditor({ editor, scale, rotationDegrees, onCommit, onCancel, onType }: TextRunEditorProps) {
  const { run, style, family } = editor;
  const [text, setText] = useState(run.text);
  const field = useRef<HTMLInputElement>(null);
  /** Set once Enter or Escape ended it: the blur that follows commits nothing more. */
  const ended = useRef(false);
  useEffect(() => {
    field.current?.focus({ preventScroll: true });
    field.current?.select();
    onType(run.text);
    // Once, on open.
  }, []);

  const box = editorBox(run.rect, rotationDegrees, scale);
  const fontPx = (style?.fontSizePt ?? run.rect.height * 0.8) * scale;
  const width = Math.max(box.width, text.length * fontPx * AVERAGE_ADVANCE);
  const face = faceOf(style, null);
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
      onChange={(event) => {
        setText(event.target.value);
        onType(event.target.value);
      }}
      // The pages' keys (arrows move the run, Delete removes it) are not this field's.
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === 'Enter') {
          event.preventDefault();
          ended.current = true;
          onCommit(text, true);
        } else if (event.key === 'Escape') {
          event.preventDefault();
          ended.current = true;
          onCancel();
        }
      }}
      onBlur={() => {
        if (ended.current) return;
        ended.current = true;
        onCommit(text, false);
      }}
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
