// The page-content model across the worker boundary (B26 T-258): the core's
// content entries (wasm objects, boxes in PDF space) become plain values on
// the page as drawn.
//
// Every rule is the core's (core/pdf-wasm/src/model/content.rs): the gates,
// the pending edits laid on, the grouping into lines and paragraphs, a run's
// style, whether a run's text decoded. This only moves the values across,
// places the boxes through `PageFrame` like every other box the engine
// hands out, and frees what it read.

import type {
  ContentFontKind,
  ContentImage,
  ContentPath,
  ContentRun,
  PageContent,
  TextBlock,
  TextLine,
  TextRunStyle,
} from '../core/types.js';
import type { PageFrame } from './annotations.js';
import type { Owned } from './pageText.js';

/** core/pdf-wasm's `ContentRunEntry`. */
export interface CoreContentRun extends Owned {
  readonly id: number;
  readonly page: number;
  readonly bbox: Float64Array;
  readonly fontName: string;
  readonly fontKind: string;
  readonly text: string;
  readonly editable: boolean;
}

/** core/pdf-wasm's `ContentImageEntry`. */
export interface CoreContentImage extends Owned {
  readonly id: number;
  readonly page: number;
  readonly bbox: Float64Array;
  readonly inline: boolean;
}

/** core/pdf-wasm's `ContentPathEntry`. */
export interface CoreContentPath extends Owned {
  readonly id: number;
  readonly page: number;
  readonly bbox: Float64Array;
  readonly paint: string;
}

/** core/pdf-wasm's `PageContentEntry`: each list moved out once. */
export interface CorePageContent extends Owned {
  takeRuns(): CoreContentRun[];
  takeImages(): CoreContentImage[];
  takePaths(): CoreContentPath[];
}

/** core/pdf-wasm's `TextLineEntry`. */
export interface CoreTextLine extends Owned {
  readonly runIds: Float64Array;
  readonly bbox: Float64Array;
  readonly text: string;
}

/** core/pdf-wasm's `TextBlockEntry`. */
export interface CoreTextBlock extends Owned {
  readonly runIds: Float64Array;
  readonly bbox: Float64Array;
  readonly text: string;
  readonly lines: CoreTextLine[];
  readonly leading: number | undefined;
  readonly alignment: string;
}

/** core/pdf-wasm's `TextRunStyleEntry`. */
export interface CoreTextRunStyle extends Owned {
  readonly baseFont: string;
  readonly fontSize: number;
  readonly fillRgb: Float64Array;
  readonly charSpacing: number;
  readonly horizontalScale: number;
  readonly bold: boolean;
  readonly italic: boolean;
  readonly renderMode: number;
}

/** Copies a page's content into plain values on the page as drawn, and frees it. */
export function pageContent(content: CorePageContent, frame: PageFrame, pageIndex: number): PageContent {
  try {
    return {
      runs: drain(content.takeRuns(), (run): ContentRun => ({
        kind: 'run',
        id: run.id,
        pageIndex,
        rect: frame.rectFromPdf(run.bbox),
        fontName: run.fontName,
        fontKind: fontKind(run.fontKind),
        text: run.text,
        editable: run.editable,
      })),
      images: drain(content.takeImages(), (image): ContentImage => ({
        kind: 'image',
        id: image.id,
        pageIndex,
        rect: frame.rectFromPdf(image.bbox),
        inline: image.inline,
      })),
      paths: drain(content.takePaths(), (path): ContentPath => ({
        kind: 'path',
        id: path.id,
        pageIndex,
        rect: frame.rectFromPdf(path.bbox),
        paint: path.paint === 'fill' || path.paint === 'fillAndStroke' ? path.paint : 'stroke',
      })),
    };
  } finally {
    content.free();
  }
}

/** Copies the core's text blocks into plain values on the page as drawn, and frees them. */
export function textBlocks(blocks: CoreTextBlock[], frame: PageFrame): TextBlock[] {
  return drain(blocks, (block): TextBlock => ({
    runIds: Array.from(block.runIds),
    rect: frame.rectFromPdf(block.bbox),
    text: block.text,
    lines: drain(block.lines, (line): TextLine => ({
      runIds: Array.from(line.runIds),
      rect: frame.rectFromPdf(line.bbox),
      text: line.text,
    })),
    leadingPt: block.leading ?? null,
    alignment: block.alignment === 'center' || block.alignment === 'right' ? block.alignment : 'left',
  }));
}

/** Copies a run's style into plain values (colour channels 0-255), and frees it. */
export function textRunStyle(style: CoreTextRunStyle | null | undefined): TextRunStyle | null {
  if (!style) return null;
  try {
    const [r = 0, g = 0, b = 0] = Array.from(style.fillRgb, (channel) => Math.round(channel * 255));
    return {
      baseFont: style.baseFont,
      fontSizePt: style.fontSize,
      color: { r, g, b },
      charSpacingPt: style.charSpacing,
      horizontalScale: style.horizontalScale,
      bold: style.bold,
      italic: style.italic,
      renderMode: style.renderMode,
    };
  } finally {
    style.free();
  }
}

/** The core's `[resource, baseFont, …]` pairs as a record. */
export function fontFamilies(pairs: readonly string[]): Record<string, string> {
  const families: Record<string, string> = {};
  for (let i = 0; i + 1 < pairs.length; i += 2) families[pairs[i]!] = pairs[i + 1]!;
  return families;
}

function fontKind(kind: string): ContentFontKind {
  return kind === 'standard14' || kind === 'embeddedSimple' ? kind : 'embeddedComposite';
}

/** Maps every entry and frees it, even when a later one throws. */
function drain<T extends Owned, U>(entries: T[], map: (entry: T) => U): U[] {
  try {
    return entries.map(map);
  } finally {
    for (const entry of entries) entry.free();
  }
}
