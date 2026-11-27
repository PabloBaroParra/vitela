// The multi-line editor over a paragraph (B26 T-261): a field laid over the
// paragraph's box, turned with the page, in its first run's size, colour and
// leading, wrapping at the paragraph's width as the core will wrap it, and
// growing down as lines are added. Keys as in the GTK shell: Enter is a line
// break, Ctrl/Cmd+Enter commits, Escape cancels, leaving it commits.
// Presentational: the draft and what a commit records are the container's
// (`useParagraphEditing`), so the draft outlives the field when its page slot
// is unmounted by a scroll (`onDetach`).

import { useEffect, useRef, type CSSProperties } from 'react';
import { paragraphKey } from './textBlocks';
import { editorBox, faceOf, fallbackFamilies } from './textRuns';
import type { ParagraphEditor } from './useParagraphEditing';

interface TextBlockEditorProps {
  readonly editor: ParagraphEditor;
  /** CSS px per PDF point. */
  readonly scale: number;
  readonly rotationDegrees: number;
  /** Ctrl+Enter (`refocus`: give the keyboard back to the pages) or leaving the field. */
  readonly onCommit: (refocus: boolean) => void;
  readonly onCancel: () => void;
  /** Each change of the text: the draft is the container's. */
  readonly onType: (text: string) => void;
  /** The field is mounted, and when it goes (its page slot unmounted, or the editor closed). */
  readonly onAttach: () => void;
  readonly onDetach: () => void;
}

/** A paragraph's line height when the core read no leading, in ems. */
const DEFAULT_LEADING_EM = 1.2;

export function TextBlockEditor({ editor, scale, rotationDegrees, onCommit, onCancel, onType, onAttach, onDetach }: TextBlockEditorProps) {
  const { block, style, draft } = editor;
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    field.current?.focus({ preventScroll: true });
    onAttach();
    return onDetach;
    // Once per mount: the callbacks are the container's, and stable.
  }, []);

  const box = editorBox(block.rect, rotationDegrees, scale);
  const sizePt = style?.fontSizePt ?? (block.lines[0]?.rect.height ?? block.rect.height) * 0.8;
  const fontPx = sizePt * scale;
  const linePx = (block.leadingPt ?? sizePt * DEFAULT_LEADING_EM) * scale;
  const lines = Math.max(block.lines.length, draft.split('\n').length);
  const face = faceOf(style, null);
  const css: CSSProperties = {
    left: box.left,
    top: box.top,
    width: box.width,
    // At least the paragraph's own box, one more line for each typed.
    height: Math.max(box.height, lines * linePx),
    transform: box.rotation ? `rotate(${box.rotation}deg)` : undefined,
    fontFamily: fallbackFamilies(style?.baseFont ?? '').map(quoteFamily).join(', '),
    fontSize: fontPx,
    lineHeight: `${linePx}px`,
    fontWeight: face.bold ? 700 : 400,
    fontStyle: face.italic ? 'italic' : 'normal',
    textAlign: block.alignment,
    color: style ? `rgb(${style.color.r} ${style.color.g} ${style.color.b})` : 'rgb(0 0 0)',
  };
  const first = block.lines[0]?.text.trim() ?? '';
  return (
    <textarea
      ref={field}
      className="text-block-editor"
      aria-label={`Edit paragraph “${first}…”`}
      value={draft}
      spellCheck={false}
      autoComplete="off"
      style={css}
      onChange={(event) => onType(event.target.value)}
      // The pages' keys (arrows move the run, Delete removes it) are not this field's.
      onKeyDown={(event) => {
        event.stopPropagation();
        const action = paragraphKey(event.key, event.ctrlKey || event.metaKey);
        if (action === 'commit') {
          event.preventDefault();
          onCommit(true);
        } else if (action === 'cancel') {
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
