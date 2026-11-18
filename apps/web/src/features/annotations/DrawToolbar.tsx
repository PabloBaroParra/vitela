// The viewer's drawing toolbar: what a press on the page does (Select, or
// one of the drawing tools, `aria-pressed`) and the colour ink and shapes
// take. Presentational.
//
// The tools are `aria-disabled`, not `disabled`, like the markup buttons: a
// button that turns disabled under the keyboard focus (while a step runs)
// would drop the focus to the page. Select is always available.

import { useId, type CSSProperties } from 'react';
import type { Rgb } from '@vitela/core';
import { DRAW_TOOLS, PALETTE, type DrawTool } from './drawing';
import { colorCss } from './geometry';

interface DrawToolbarProps {
  readonly tool: DrawTool;
  readonly onTool: (tool: DrawTool) => void;
  readonly color: Rgb;
  readonly onColor: (color: Rgb) => void;
  /** `false` when the document forbids annotations: the tools say why. */
  readonly allowed: boolean;
  /** `false` while an edit, undo or redo is still running. */
  readonly ready: boolean;
}

const sameColor = (a: Rgb, b: Rgb) => a.r === b.r && a.g === b.g && a.b === b.b;

export function DrawToolbar({ tool, onTool, color, onColor, allowed, ready }: DrawToolbarProps) {
  const blockedId = useId();
  const canArm = allowed && ready;
  return (
    <div className="viewer-toolbar draw-toolbar" role="toolbar" aria-label="Draw">
      <span className="draw-toolbar__tools" role="group" aria-label="Tool">
        {DRAW_TOOLS.map(({ tool: option, label }) => {
          const enabled = option === 'select' || canArm;
          return (
            <button
              key={option}
              type="button"
              className="draw-tool"
              data-tool={option}
              aria-pressed={tool === option}
              aria-disabled={!enabled}
              aria-describedby={option === 'select' || allowed ? undefined : blockedId}
              onClick={enabled ? () => onTool(option) : undefined}
            >
              {label}
            </button>
          );
        })}
      </span>
      <span className="draw-toolbar__colors" role="group" aria-label="Colour">
        {PALETTE.map(({ name, color: swatch }) => (
          <button
            key={name}
            type="button"
            className="draw-color"
            aria-label={name}
            aria-pressed={sameColor(color, swatch)}
            style={{ '--swatch': colorCss(swatch) } as CSSProperties}
            onClick={() => onColor(swatch)}
          />
        ))}
      </span>
      {!allowed && (
        <p id={blockedId} className="text-toolbar__blocked" role="note">
          This PDF does not allow annotations, so Ink, Shape, Note and Free text are turned off.
        </p>
      )}
    </div>
  );
}
