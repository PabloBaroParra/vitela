// A text box's text as the core lays it out, in the box's own frame (before
// the page's turn), at the box's size in points. Presentational.

import type { FreeTextLayout } from '@vitela/core';
import type { Size } from './drawing';

interface FreeTextPreviewProps {
  readonly frame: Size;
  readonly layout: FreeTextLayout | null;
}

export function FreeTextPreview({ frame, layout }: FreeTextPreviewProps) {
  return (
    <figure className="free-text-preview">
      <svg
        viewBox={`0 0 ${frame.width} ${frame.height}`}
        className="free-text-preview__box annotation-mark--free-text"
        role="img"
        aria-label="Text box preview"
      >
        <rect className="free-text-preview__frame" width={frame.width} height={frame.height} />
        {layout?.lines.map((line, index) => (
          <text key={index} x={line.xPt} y={line.baselineFromTopPt} fontSize={layout.fontSizePt}>
            {line.text}
          </text>
        ))}
      </svg>
      <figcaption className="free-text-preview__caption">
        {layout?.overflow ? 'The text runs past the box; it will be cut off.' : 'Preview, as it will be saved.'}
      </figcaption>
    </figure>
  );
}
