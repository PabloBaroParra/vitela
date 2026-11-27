// Domain types of a page's content and its edits (B26 T-258b, T-259..T-262):
// what a page paints as the core reads it, how a run looks, the font program
// it embeds, the edits of a run, a text block, an image or a selection of
// them, and the content clipboard's clip. Re-exported by `types.ts` (the
// engine's interface uses them) and by the package entry.

import type { PagePoint, PageRect, Rgb } from './types.js';

/** How a text run's font is held: the 14 standard fonts, an embedded simple font, or a composite (CID) font. */
export type ContentFontKind = 'standard14' | 'embeddedSimple' | 'embeddedComposite';

/**
 * One run of text a page paints, as the reader sees it (every pending
 * content edit laid on). `id` is valid against the read it came from: read
 * again after an edit, or when `contentBaseGeneration` changes.
 */
export interface ContentRun {
  readonly kind: 'run';
  readonly id: number;
  readonly pageIndex: number;
  /** On the page as drawn (top-left origin, `/Rotate` applied). */
  readonly rect: PageRect;
  /** The font's key in the page's resources (`F1`); its `/BaseFont` is in `pageFontFamilies`. */
  readonly fontName: string;
  readonly fontKind: ContentFontKind;
  readonly text: string;
  /** Whether its text decoded fully, so it could be retyped (the core's rule). */
  readonly editable: boolean;
  /** Whether it exists only because of an unsaved edit: an insertion or a paste not saved yet. */
  readonly pending: boolean;
}

/** One image a page paints. */
export interface ContentImage {
  readonly kind: 'image';
  readonly id: number;
  readonly pageIndex: number;
  readonly rect: PageRect;
  /** Its samples sit in the content stream rather than in an XObject. */
  readonly inline: boolean;
  /** Whether it exists only because of an unsaved edit. */
  readonly pending: boolean;
}

/** One vector path a page paints: a rule, a border, a shaded box. */
export interface ContentPath {
  readonly kind: 'path';
  readonly id: number;
  readonly pageIndex: number;
  /** The geometry's box without the stroke's width: a hairline rule's is zero high. */
  readonly rect: PageRect;
  readonly paint: 'stroke' | 'fill' | 'fillAndStroke';
  /** Whether it exists only because of an unsaved edit. */
  readonly pending: boolean;
}

export type ContentItem = ContentRun | ContentImage | ContentPath;

/** Everything one page paints, each kind in paint order. */
export interface PageContent {
  readonly runs: readonly ContentRun[];
  readonly images: readonly ContentImage[];
  readonly paths: readonly ContentPath[];
}

/** One line of a `TextBlock`. */
export interface TextLine {
  /** The member runs' ids, left to right. */
  readonly runIds: readonly number[];
  readonly rect: PageRect;
  readonly text: string;
}

/** Runs a reader sees as one piece of text: one line, or a paragraph of stacked lines. */
export interface TextBlock {
  /** The member runs' ids, line by line from the top. */
  readonly runIds: readonly number[];
  readonly rect: PageRect;
  /** The lines' text joined with a line break. */
  readonly text: string;
  readonly lines: readonly TextLine[];
  /** The mean baseline-to-baseline distance in points; `null` for one line. */
  readonly leadingPt: number | null;
  readonly alignment: 'left' | 'center' | 'right';
}

/** How a run looks on its page (the core's reading of the file). */
export interface TextRunStyle {
  /** The font's `/BaseFont`, raw (`ABCDEF+Vera-Bold`); empty when it names none. */
  readonly baseFont: string;
  /** The size a reader sees, in points. */
  readonly fontSizePt: number;
  /** The fill colour. */
  readonly color: Rgb;
  /** Extra space after each glyph, in points. */
  readonly charSpacingPt: number;
  /** 1 is unscaled. */
  readonly horizontalScale: number;
  readonly bold: boolean;
  readonly italic: boolean;
  /** The text rendering mode: 0 fills, 2 fills and strokes (a faked bold). */
  readonly renderMode: number;
}

/**
 * The four face toggles a content editor offers for a run: what the reader
 * should see, not how it is painted (the core picks a sibling face or
 * synthesizes one when it writes the change).
 */
export interface TextRunLook {
  readonly bold: boolean;
  readonly italic: boolean;
  readonly underline: boolean;
  readonly strike: boolean;
}

/** How a run looks now (a queued restyle included), and which styles can come off it. */
export interface TextRunLookState extends TextRunLook {
  /** `false` only for a run painted in a bold font whose page has no regular sibling: un-bolding is never faked. */
  readonly canRemoveBold: boolean;
  /** The same, for italic. */
  readonly canRemoveItalic: boolean;
}

/** What kind of program a font embeds: a browser's `FontFace` takes `trueType` and `openType`. */
export type FontProgramFormat = 'trueType' | 'openType' | 'cff' | 'type1';

/** A font program the file embeds, decoded. */
export interface FontProgram {
  readonly format: FontProgramFormat;
  readonly bytes: Uint8Array;
}

/**
 * An edit of a text run a page's content read handed out (`runId`, valid
 * against that read), on page position `page`. Page content: one undo step
 * that the pixels show at once and `paintRevision` moves for. A second
 * retype or move of the same run folds into its step; an edit that changes
 * nothing (the same text, a zero move, the look the runs have) records
 * nothing. `dx, dy` are on the page as drawn (points, y down). A restyle
 * gives every run of `runIds` the look `look`, as one step. Rejects as
 * `contentEditingRefusal` says (`EditNotPermitted`, or
 * `TextExtractionDenied`), then `PageIndexOutOfBounds`,
 * `UnsupportedOperation` for a run the page does not hold now, or
 * `EncodingGap` for a character the run's font cannot show.
 */
export type TextRunEdit =
  | { readonly kind: 'retypeTextRun'; readonly page: number; readonly runId: number; readonly text: string }
  | { readonly kind: 'moveTextRun'; readonly page: number; readonly runId: number; readonly dx: number; readonly dy: number }
  | { readonly kind: 'removeTextRun'; readonly page: number; readonly runId: number }
  | { readonly kind: 'restyleTextRuns'; readonly page: number; readonly runIds: readonly number[]; readonly look: TextRunLook };

/**
 * An edit of text as a block (T-261), on page position `page`. Page content:
 * one undo step that the pixels show at once and `paintRevision` moves for.
 *
 * - `retypeTextBlock` shows `text` in place of the block made of `runIds`
 *   — a block's runs exactly as `readParagraphBlocks` lists them, valid
 *   against that read. A paragraph wraps every typed line to its width
 *   (each line break is the reader's), lines it grows painted like its own;
 *   a one-line block puts all of the text in its first run; text with
 *   nothing to show removes the block. A second retype of the same block
 *   folds into its step; the text it already shows records nothing.
 * - `insertText` paints `text`, one line, in Helvetica at `sizePt` (1–72),
 *   its top-left corner at `at` (on the page as drawn, points, y down).
 *
 * Rejects as `contentEditingRefusal` says (`EditNotPermitted`, or
 * `TextExtractionDenied`), then `PageIndexOutOfBounds`,
 * `UnsupportedOperation` for runs that are not one block now, or an
 * insertion with nothing to show, a line break or a size out of range, or
 * `EncodingGap` for a character the font cannot show.
 */
export type TextBlockEdit =
  | { readonly kind: 'retypeTextBlock'; readonly page: number; readonly runIds: readonly number[]; readonly text: string }
  | { readonly kind: 'insertText'; readonly page: number; readonly at: PagePoint; readonly text: string; readonly sizePt: number };

/**
 * An edit of an image a page's content read handed out (`imageId`, valid
 * against that read), on page position `page`, or of a new one. Page
 * content: one undo step that the pixels show at once and `paintRevision`
 * moves for. A second move or resize of the same image folds into its step;
 * one that changes nothing (a zero move, the box it has) records nothing.
 * Geometry is on the page as drawn (points, y down): `dx, dy` a move, `rect`
 * the box a resize paints it into, `at` the top-left corner of an inserted
 * image, whose size is the core's (its own proportions, the longest side two
 * inches). `bytes` are an encoded PNG or JPEG; ask `checkImageReplacement`
 * before a replacement is chosen. Rejects as `contentEditingRefusal` says
 * (`EditNotPermitted`, or `TextExtractionDenied`), then
 * `PageIndexOutOfBounds`, `UnsupportedOperation` for an image the page does
 * not hold now or a box with no area, or `InvalidImage` for bytes that do
 * not decode.
 */
export type ImageEdit =
  | { readonly kind: 'moveImage'; readonly page: number; readonly imageId: number; readonly dx: number; readonly dy: number }
  | { readonly kind: 'resizeImage'; readonly page: number; readonly imageId: number; readonly rect: PageRect }
  | { readonly kind: 'removeImage'; readonly page: number; readonly imageId: number }
  | { readonly kind: 'replaceImage'; readonly page: number; readonly imageId: number; readonly bytes: Uint8Array }
  | { readonly kind: 'insertImage'; readonly page: number; readonly at: PagePoint; readonly bytes: Uint8Array };

/**
 * An edit of a vector path a page's content read handed out (`pathId`,
 * valid against that read), on page position `page`: a rule, a border, a
 * shaded box. Page content: one undo step that the pixels show at once and
 * `paintRevision` moves for. A second move or resize of the same path, or
 * its removal, folds into its step; one that changes nothing (a zero move,
 * the box it has) records nothing. Geometry is on the page as drawn (points,
 * y down): `dx, dy` a move, `rect` the box a resize reshapes it into. A
 * resize keeps the line width; a side the path is flat along (a rule's
 * height) stays flat whatever `rect` gives it, and one it has may not go to
 * nothing. Rejects as `contentEditingRefusal` says (`EditNotPermitted`, or
 * `TextExtractionDenied`), then `PageIndexOutOfBounds`, or
 * `UnsupportedOperation` for a path the page does not hold now or a box
 * that takes a side it has to nothing.
 */
export type PathEdit =
  | { readonly kind: 'movePath'; readonly page: number; readonly pathId: number; readonly dx: number; readonly dy: number }
  | { readonly kind: 'resizePath'; readonly page: number; readonly pathId: number; readonly rect: PageRect }
  | { readonly kind: 'removePath'; readonly page: number; readonly pathId: number };

/** The members of a selection on one page, by the ids that page's content read handed out. */
export interface ContentMembers {
  readonly runIds: readonly number[];
  readonly imageIds: readonly number[];
  readonly pathIds: readonly number[];
}

/**
 * Content copied out of a page (`copyContentSelection`): a self-contained
 * one-page PDF the session holds on to, so it outlives the page — and the
 * document — it came from. Hand it back unchanged to paste it.
 */
export interface ContentClip {
  readonly pdf: Uint8Array;
  /** The copied items' box as the core keeps it: `[x, y, width, height]` in the source page's PDF points (y up). */
  readonly bbox: readonly [number, number, number, number];
  /** The same box on the source page as drawn (points, y down). */
  readonly rect: PageRect;
  /** Whether the session it came from held anything from an encrypted PDF (the core's provenance, handed back with the rest). */
  readonly fromEncrypted: boolean;
}

/**
 * An edit of several content items at once, or a paste (T-262), on page
 * position `page`. Page content: one undo step that the pixels show at once
 * and `paintRevision` moves for.
 *
 * - `moveContentSelection` shifts every member by `dx, dy` (on the page as
 *   drawn, points, y down). A second move of the same selection folds into
 *   its step; a zero move records nothing. Everything one pending paste
 *   added moves the paste itself; part of one is refused until a save.
 * - `removeContentSelection` takes every member off the page.
 * - `pasteContent` writes `clip` onto the page, its box's top-left corner at
 *   `at` (on the page as drawn, turned with the page about that corner).
 *   What it adds reads back `pending` until the document is saved.
 *
 * Rejects as `contentEditingRefusal` says (`EditNotPermitted`, or
 * `TextExtractionDenied` for a selection edit), then `PageIndexOutOfBounds`,
 * or `UnsupportedOperation` for an empty selection, an item the page does
 * not hold now, or part of a pending paste.
 */
export type SelectionEdit =
  | { readonly kind: 'moveContentSelection'; readonly page: number; readonly members: ContentMembers; readonly dx: number; readonly dy: number }
  | { readonly kind: 'removeContentSelection'; readonly page: number; readonly members: ContentMembers }
  | { readonly kind: 'pasteContent'; readonly page: number; readonly clip: ContentClip; readonly at: PagePoint };
