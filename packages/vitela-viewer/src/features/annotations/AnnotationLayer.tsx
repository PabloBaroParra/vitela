// The annotation layer over one page: draws the session's annotations from
// the engine's snapshot. They are not in pdfium's bitmap (the preview leaves
// the session's annotations out until a save writes them), so the shell
// draws them, as the native shells do. Lives inside the page's slot, under
// the text layer, and mounts and unmounts with the page.
//
// One SVG whose units are the page's own drawn points: the snapshot's boxes
// go in as they are and the zoom is only the SVG's CSS size. Presentational,
// and inert to the pointer: text selection works through it.

import { memo, useId } from 'react';
import type { Annotation, PageSize } from '@vitela/core';
import { colorCss, freeTextFrame, markupRule, RULE_PT } from './geometry';

/** How opaque markup, shapes and ink are drawn, as the native shells draw them. */
export const ANNOTATION_OPACITY = 0.45;
/** Outline of a shape and the width of an ink stroke, in points (the saved default border). */
const STROKE_PT = 1;

interface AnnotationLayerProps {
  /** The page as drawn: its size in points and its turn. */
  readonly page: PageSize;
  /** This page's annotations, in paint order. */
  readonly annotations: readonly Annotation[];
  /** The image of each stamp added this session, by annotation id (the engine hands none back). */
  readonly stampImages?: ReadonlyMap<number, string>;
}

const NO_IMAGES: ReadonlyMap<number, string> = new Map();

export const AnnotationLayer = memo(function AnnotationLayer({ page, annotations, stampImages = NO_IMAGES }: AnnotationLayerProps) {
  // Only characters that need no escaping inside `url(#…)`.
  const clipPrefix = `clip${useId().replace(/[^\w-]/g, '')}`;
  return (
    <svg
      className="annotation-layer"
      viewBox={`0 0 ${page.widthPt} ${page.heightPt}`}
      preserveAspectRatio="none"
      aria-hidden="true"
    >
      {annotations.map((annotation) => (
        <Mark
          key={annotation.id}
          annotation={annotation}
          rotation={page.rotationDegrees}
          clipId={`${clipPrefix}-${annotation.id}`}
          image={annotation.kind === 'stamp' ? stampImages.get(annotation.id) : undefined}
        />
      ))}
    </svg>
  );
});

interface MarkProps {
  readonly annotation: Annotation;
  readonly rotation: number;
  readonly clipId: string;
  /** A stamp's image, when this session kept one for it. */
  readonly image: string | undefined;
}

function Mark({ annotation, rotation, clipId, image }: MarkProps) {
  const tag = { 'data-annotation-id': annotation.id, 'data-kind': annotation.kind };
  switch (annotation.kind) {
    case 'highlight': {
      const { left, top, width, height } = annotation.rect;
      return (
        <rect
          {...tag}
          className="annotation-mark annotation-mark--highlight"
          x={left}
          y={top}
          width={width}
          height={height}
          fill={colorCss(annotation.color)}
          fillOpacity={ANNOTATION_OPACITY}
        />
      );
    }
    case 'underline':
    case 'strikeout': {
      const { x1, y1, x2, y2 } = markupRule(annotation.kind, annotation.rect, rotation);
      return (
        <line
          {...tag}
          className="annotation-mark"
          x1={x1}
          y1={y1}
          x2={x2}
          y2={y2}
          stroke={colorCss(annotation.color)}
          strokeWidth={RULE_PT}
        />
      );
    }
    case 'shape': {
      const { left, top, width, height } = annotation.rect;
      return (
        <rect
          {...tag}
          className="annotation-mark"
          x={left}
          y={top}
          width={width}
          height={height}
          fill="none"
          stroke={colorCss(annotation.color)}
          strokeWidth={STROKE_PT}
        />
      );
    }
    case 'ink':
      return (
        <polyline
          {...tag}
          className="annotation-mark"
          points={annotation.points.map(({ x, y }) => `${x},${y}`).join(' ')}
          fill="none"
          stroke={colorCss(annotation.color)}
          strokeWidth={STROKE_PT}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      );
    case 'note': {
      const { left, top, width, height } = annotation.rect;
      // A sticky-note icon; the note's text is its tooltip until the comments panel (T-241).
      return (
        <g {...tag} className="annotation-mark annotation-mark--note">
          <title>{annotation.contents}</title>
          <rect x={left} y={top} width={width} height={height} rx={Math.min(width, height) / 8} />
          {[0.3, 0.5, 0.7].map((at) => (
            <line key={at} x1={left + width * 0.2} y1={top + height * at} x2={left + width * 0.8} y2={top + height * at} />
          ))}
        </g>
      );
    }
    case 'stamp': {
      if (image) {
        // Upright in the box's own frame, turned with the page like free text
        // (and like the saved appearance). The core gave the box the image's
        // proportions, so the image fills it exactly.
        const frame = freeTextFrame(annotation.rect, rotation);
        return (
          <g {...tag} className="annotation-mark annotation-mark--stamp-image" transform={frame.transform}>
            <image href={image} width={frame.width} height={frame.height} preserveAspectRatio="none" />
          </g>
        );
      }
      // No image kept for it (the engine hands none back): a placeholder box.
      const { left, top, width, height } = annotation.rect;
      return (
        <rect
          {...tag}
          className="annotation-mark annotation-mark--stamp"
          x={left}
          y={top}
          width={width}
          height={height}
        />
      );
    }
    case 'freeText': {
      // The core's layout, in the box's own frame; opaque, like the saved appearance.
      const frame = freeTextFrame(annotation.rect, rotation);
      const { layout } = annotation;
      return (
        <g {...tag} className="annotation-mark annotation-mark--free-text" transform={frame.transform}>
          <clipPath id={clipId}>
            <rect width={frame.width} height={frame.height} />
          </clipPath>
          <g clipPath={`url(#${clipId})`}>
            {layout.lines.map((line, index) => (
              <text key={index} x={line.xPt} y={line.baselineFromTopPt} fontSize={layout.fontSizePt}>
                {line.text}
              </text>
            ))}
          </g>
        </g>
      );
    }
  }
}
