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
  ContentClip,
  ContentFontKind,
  ContentMembers,
  SelectionEdit,
  FontProgram,
  ImageEdit,
  PathEdit,
  TextBlockEdit,
  TextRunEdit,
  TextRunLookState,
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
  readonly pending: boolean;
}

/** core/pdf-wasm's `ContentImageEntry`. */
export interface CoreContentImage extends Owned {
  readonly id: number;
  readonly page: number;
  readonly bbox: Float64Array;
  readonly inline: boolean;
  readonly pending: boolean;
}

/** core/pdf-wasm's `ContentPathEntry`. */
export interface CoreContentPath extends Owned {
  readonly id: number;
  readonly page: number;
  readonly bbox: Float64Array;
  readonly paint: string;
  readonly pending: boolean;
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

/** core/pdf-wasm's text-run `EditCommand` constructors (T-259). */
export interface CoreTextRunEdits {
  retypeTextRun(page: number, runId: number, text: string): Owned;
  /** `dx, dy` in PDF points (y up). */
  moveTextRun(page: number, runId: number, dx: number, dy: number): Owned;
  removeTextRun(page: number, runId: number): Owned;
  restyleTextRuns(page: number, runIds: Float64Array, bold: boolean, italic: boolean, underline: boolean, strike: boolean): Owned;
}

/** core/pdf-wasm's text-block `EditCommand` constructors (T-261); geometry in PDF points (y up). */
export interface CoreTextBlockEdits {
  retypeTextBlock(page: number, runIds: Float64Array, text: string): Owned;
  insertText(page: number, x: number, y: number, text: string, size: number): Owned;
}

/** core/pdf-wasm's image `EditCommand` constructors (T-260); geometry in PDF points (y up). */
export interface CoreImageEdits {
  moveImage(page: number, imageId: number, dx: number, dy: number): Owned;
  resizeImage(page: number, imageId: number, x: number, y: number, width: number, height: number): Owned;
  removeImage(page: number, imageId: number): Owned;
  replaceImage(page: number, imageId: number, bytes: Uint8Array): Owned;
  insertImage(page: number, x: number, y: number, bytes: Uint8Array): Owned;
}

/** core/pdf-wasm's vector-path `EditCommand` constructors (T-263); geometry in PDF points (y up). */
export interface CorePathEdits {
  movePath(page: number, pathId: number, dx: number, dy: number): Owned;
  resizePath(page: number, pathId: number, x: number, y: number, width: number, height: number): Owned;
  removePath(page: number, pathId: number): Owned;
}

/** core/pdf-wasm's selection and paste `EditCommand` constructors (T-262); geometry in PDF points (y up). */
export interface CoreSelectionEdits {
  moveContentSelection(page: number, runIds: Float64Array, imageIds: Float64Array, pathIds: Float64Array, dx: number, dy: number): Owned;
  removeContentSelection(page: number, runIds: Float64Array, imageIds: Float64Array, pathIds: Float64Array): Owned;
  pasteContent(page: number, pdf: Uint8Array, bbox: Float64Array, fromEncrypted: boolean, left: number, top: number): Owned;
}

/** core/pdf-wasm's `ContentClipEntry`. */
export interface CoreContentClip extends Owned {
  readonly bbox: Float64Array;
  readonly fromEncrypted: boolean;
  takePdf(): Uint8Array;
}

/** core/pdf-wasm's `TextRunLookEntry`. */
export interface CoreTextRunLook extends Owned {
  readonly bold: boolean;
  readonly italic: boolean;
  readonly underline: boolean;
  readonly strike: boolean;
  readonly canRemoveBold: boolean;
  readonly canRemoveItalic: boolean;
}

/** core/pdf-wasm's `FontProgramEntry`. */
export interface CoreFontProgram extends Owned {
  readonly format: string;
  takeBytes(): Uint8Array;
}

/**
 * The core command for `edit`. A move's delta is on the page as drawn and
 * crosses through the page's frame; on a page past the end it crosses as is,
 * for the core to refuse (`PageIndexOutOfBounds`) after its own gates.
 */
export function textRunCommand(edits: CoreTextRunEdits, edit: TextRunEdit, frameOf: (page: number) => PageFrame | undefined): Owned {
  switch (edit.kind) {
    case 'retypeTextRun':
      return edits.retypeTextRun(edit.page, edit.runId, edit.text);
    case 'moveTextRun': {
      const [dx, dy] = frameOf(edit.page)?.deltaToPdf(edit.dx, edit.dy) ?? [edit.dx, edit.dy];
      return edits.moveTextRun(edit.page, edit.runId, dx, dy);
    }
    case 'removeTextRun':
      return edits.removeTextRun(edit.page, edit.runId);
    case 'restyleTextRuns': {
      const { bold, italic, underline, strike } = edit.look;
      return edits.restyleTextRuns(edit.page, Float64Array.from(edit.runIds), bold, italic, underline, strike);
    }
  }
}

/**
 * The core command for a text-block `edit`. An insertion's point is on the
 * page as drawn and crosses through the page's frame; on a page past the end
 * it crosses as is, for the core to refuse (`PageIndexOutOfBounds`) after
 * its own gates.
 */
export function textBlockCommand(edits: CoreTextBlockEdits, edit: TextBlockEdit, frameOf: (page: number) => PageFrame | undefined): Owned {
  switch (edit.kind) {
    case 'retypeTextBlock':
      return edits.retypeTextBlock(edit.page, Float64Array.from(edit.runIds), edit.text);
    case 'insertText': {
      const [x, y] = frameOf(edit.page)?.pointToPdf(edit.at) ?? [edit.at.x, edit.at.y];
      return edits.insertText(edit.page, x, y, edit.text, edit.sizePt);
    }
  }
}

/**
 * The core command for an image `edit`. Its geometry is on the page as
 * drawn and crosses through the page's frame; on a page past the end it
 * crosses as is, for the core to refuse (`PageIndexOutOfBounds`) after its
 * own gates.
 */
export function imageCommand(edits: CoreImageEdits, edit: ImageEdit, frameOf: (page: number) => PageFrame | undefined): Owned {
  const frame = frameOf(edit.page);
  switch (edit.kind) {
    case 'moveImage': {
      const [dx, dy] = frame?.deltaToPdf(edit.dx, edit.dy) ?? [edit.dx, edit.dy];
      return edits.moveImage(edit.page, edit.imageId, dx, dy);
    }
    case 'resizeImage': {
      const { left, top, width, height } = edit.rect;
      const [x, y, w, h] = frame?.rectToPdf(edit.rect) ?? [left, top, width, height];
      return edits.resizeImage(edit.page, edit.imageId, x, y, w, h);
    }
    case 'removeImage':
      return edits.removeImage(edit.page, edit.imageId);
    case 'replaceImage':
      return edits.replaceImage(edit.page, edit.imageId, edit.bytes);
    case 'insertImage': {
      const [x, y] = frame?.pointToPdf(edit.at) ?? [edit.at.x, edit.at.y];
      return edits.insertImage(edit.page, x, y, edit.bytes);
    }
  }
}

/**
 * The core command for a vector-path `edit`. Its geometry is on the page as
 * drawn and crosses through the page's frame; on a page past the end it
 * crosses as is, for the core to refuse (`PageIndexOutOfBounds`) after its
 * own gates.
 */
export function pathCommand(edits: CorePathEdits, edit: PathEdit, frameOf: (page: number) => PageFrame | undefined): Owned {
  const frame = frameOf(edit.page);
  switch (edit.kind) {
    case 'movePath': {
      const [dx, dy] = frame?.deltaToPdf(edit.dx, edit.dy) ?? [edit.dx, edit.dy];
      return edits.movePath(edit.page, edit.pathId, dx, dy);
    }
    case 'resizePath': {
      const { left, top, width, height } = edit.rect;
      const [x, y, w, h] = frame?.rectToPdf(edit.rect) ?? [left, top, width, height];
      return edits.resizePath(edit.page, edit.pathId, x, y, w, h);
    }
    case 'removePath':
      return edits.removePath(edit.page, edit.pathId);
  }
}

/** A selection's ids as the core's constructors take them. */
export function memberIds(members: ContentMembers): [Float64Array, Float64Array, Float64Array] {
  return [Float64Array.from(members.runIds), Float64Array.from(members.imageIds), Float64Array.from(members.pathIds)];
}

/**
 * The core command for a selection `edit` or a paste. A move's delta and a
 * paste's point are on the page as drawn and cross through the page's frame;
 * on a page past the end they cross as is, for the core to refuse
 * (`PageIndexOutOfBounds`) after its own gates. A clip crosses back exactly
 * as the core handed it out.
 */
export function selectionCommand(edits: CoreSelectionEdits, edit: SelectionEdit, frameOf: (page: number) => PageFrame | undefined): Owned {
  switch (edit.kind) {
    case 'moveContentSelection': {
      const [dx, dy] = frameOf(edit.page)?.deltaToPdf(edit.dx, edit.dy) ?? [edit.dx, edit.dy];
      return edits.moveContentSelection(edit.page, ...memberIds(edit.members), dx, dy);
    }
    case 'removeContentSelection':
      return edits.removeContentSelection(edit.page, ...memberIds(edit.members));
    case 'pasteContent': {
      const { clip } = edit;
      const [left, top] = frameOf(edit.page)?.pointToPdf(edit.at) ?? [edit.at.x, edit.at.y];
      return edits.pasteContent(edit.page, clip.pdf, Float64Array.from(clip.bbox), clip.fromEncrypted, left, top);
    }
  }
}

/** Moves a clip's PDF out into plain values, its box also drawn on its page, and frees it. */
export function contentClip(clip: CoreContentClip, frame: PageFrame): ContentClip {
  try {
    const [x = 0, y = 0, width = 0, height = 0] = Array.from(clip.bbox);
    return { pdf: clip.takePdf(), bbox: [x, y, width, height], rect: frame.rectFromPdf(clip.bbox), fromEncrypted: clip.fromEncrypted };
  } finally {
    clip.free();
  }
}

/** Copies a run's look into plain values, and frees it. */
export function textRunLook(look: CoreTextRunLook | null | undefined): TextRunLookState | null {
  if (!look) return null;
  try {
    const { bold, italic, underline, strike, canRemoveBold, canRemoveItalic } = look;
    return { bold, italic, underline, strike, canRemoveBold, canRemoveItalic };
  } finally {
    look.free();
  }
}

/** Moves a font program's bytes out, and frees it. */
export function fontProgram(program: CoreFontProgram | null | undefined): FontProgram | null {
  if (!program) return null;
  try {
    const format = program.format;
    return {
      format: format === 'trueType' || format === 'openType' || format === 'cff' ? format : 'type1',
      bytes: program.takeBytes(),
    };
  } finally {
    program.free();
  }
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
        pending: run.pending,
      })),
      images: drain(content.takeImages(), (image): ContentImage => ({
        kind: 'image',
        id: image.id,
        pageIndex,
        rect: frame.rectFromPdf(image.bbox),
        inline: image.inline,
        pending: image.pending,
      })),
      paths: drain(content.takePaths(), (path): ContentPath => ({
        kind: 'path',
        id: path.id,
        pageIndex,
        rect: frame.rectFromPdf(path.bbox),
        paint: path.paint === 'fill' || path.paint === 'fillAndStroke' ? path.paint : 'stroke',
        pending: path.pending,
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
