// Domain types shared by the UI and the engine worker.

import type {
  PageContent,
  TextBlock,
  TextRunStyle,
  TextRunLookState,
  FontProgram,
  TextRunEdit,
  TextBlockEdit,
  ImageEdit,
  ContentClip,
  ContentMembers,
  SelectionEdit,
} from './content.js';

// Declared in `content.ts`; re-exported here for the worker and the client.
export type {
  ContentFontKind,
  ContentRun,
  ContentImage,
  ContentPath,
  ContentItem,
  PageContent,
  TextLine,
  TextBlock,
  TextRunStyle,
  TextRunLook,
  TextRunLookState,
  FontProgramFormat,
  FontProgram,
  TextRunEdit,
  TextBlockEdit,
  ImageEdit,
  ContentClip,
  ContentMembers,
  SelectionEdit,
} from './content.js';

/**
 * One page's size in PDF points as the viewer lays it out — already turned
 * by its rotation, so a `/Rotate 90` portrait page reports landscape — plus
 * that clockwise turn (0, 90, 180 or 270). Straight from the core's
 * `pageDimensions`.
 */
export interface PageSize {
  readonly widthPt: number;
  readonly heightPt: number;
  readonly rotationDegrees: number;
}

/** What the engine reports about a freshly opened document. */
export interface DocumentInfo {
  readonly pageCount: number;
  /** One entry per page, in page order. */
  readonly pages: readonly PageSize[];
  /**
   * Each page's identity (the core's `PageId`), in page order. It follows
   * the page through moves and removals, so per-page state such as a
   * thumbnail is kept by id, never by position.
   */
  readonly pageIds: readonly number[];
  /**
   * Whether the document lets its text be read (PDF permission bit 5, or
   * opened with the owner password). When `false` the engine refuses every
   * text read with `TextExtractionDenied`, and the UI offers neither search
   * nor selection.
   */
  readonly textExtractionAllowed: boolean;
  /**
   * Whether the document lets its pages be moved, rotated or removed (the
   * PDF document-assembly permission, or the owner password). When `false`
   * the engine refuses every page edit with `EditNotPermitted`.
   */
  readonly documentAssemblyAllowed: boolean;
  /**
   * The document's pages grouped into blocks — one per contiguous run of
   * pages from the same PDF (the core's `derive_blocks`), in page order. What
   * Organize's Documents view shows.
   */
  readonly blocks: readonly DocumentBlock[];
  /**
   * Why the document would refuse any import right now, in the reader's
   * words, or `null` when it would not (the core's `import_refusal`: the
   * modify-contents and assembly permissions, and whether its encryption
   * survives the rewrite an import forces). Asked before the file picker.
   */
  readonly importRefusal: string | null;
  /**
   * Whether the document lets annotations be added, changed or removed (PDF
   * permission bit 6, or the owner password). When `false` the engine
   * refuses every annotation edit with `EditNotPermitted`; reading the
   * annotations and comments still works.
   */
  readonly annotationEditingAllowed: boolean;
  /**
   * Whether the document lets its form fields be filled in (PDF permission
   * bit 6, the same bit annotating needs, or the owner password). When
   * `false` the engine refuses every `setFieldValue` with `EditNotPermitted`;
   * reading the fields still works.
   */
  readonly formFillAllowed: boolean;
}

/** Which PDF a block's pages come from. */
export type BlockSource =
  /** The PDF the document was opened from. */
  | { readonly kind: 'base' }
  /** Inserted blank pages. */
  | { readonly kind: 'blank' }
  /** A PDF added by `importPrepared`; `id` is one of its `sourceIds`. */
  | { readonly kind: 'imported'; readonly id: number };

/** One contiguous run of pages from one PDF. Positions are current until the next edit. */
export interface DocumentBlock {
  readonly source: BlockSource;
  /** 1-based, only when the PDF is split across more than one block; else `null`. */
  readonly part: number | null;
  /** Position of the block's first page. */
  readonly start: number;
  /** How many pages it holds (never zero). */
  readonly count: number;
}

/** One PDF opened as an import source, waiting for `importPrepared`. */
export interface PreparedImport {
  /** What `importPrepared` and `discardImport` name it by. */
  readonly id: number;
  /** How many pages importing it adds: every page it has. */
  readonly pageCount: number;
  /** What its pages will not bring across exactly as they are, one line each. */
  readonly warnings: readonly string[];
}

/** The document after an import, plus the block source id of each imported PDF, in pick order. */
export interface ImportedDocument extends EditedDocument {
  readonly sourceIds: readonly number[];
}

/**
 * One page edit, recorded as one undo step. Positions are 0-based and name
 * pages as they are now. A move, rotate or remove acts on the run of `count`
 * pages starting at `from`; an insert names no run, only the position `at`
 * its one new page takes.
 */
export type PageEdit =
  /** Moves the run so its first page ends up at `to`. */
  | { readonly kind: 'move'; readonly from: number; readonly count: number; readonly to: number }
  /** Turns each page of the run clockwise by `degrees` (a multiple of 90). */
  | { readonly kind: 'rotate'; readonly from: number; readonly count: number; readonly degrees: number }
  | { readonly kind: 'remove'; readonly from: number; readonly count: number }
  /**
   * Inserts one blank A4 page so it ends up at position `at` (`pageCount`
   * appends), as the native shells do. The new page gets its own page id.
   */
  | { readonly kind: 'insertBlank'; readonly at: number; readonly orientation: PageOrientation };

/** Which way a new blank page stands. */
export type PageOrientation = 'portrait' | 'landscape';

/** The document after an edit, undo or redo, and where its history stands. */
export interface EditedDocument {
  readonly info: DocumentInfo;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
  /**
   * The session's annotations as they are now, in paint order (the core's
   * snapshot). Sent with every answer because any step can change them: a
   * page move moves the annotations on it, an undo takes one away.
   */
  readonly annotations: readonly Annotation[];
  /** The id of the annotation this edit added; `null` for every other step. */
  readonly addedAnnotationId: number | null;
  /**
   * The document's form fields as they are now (what `formFields` answers).
   * Sent with every answer for the same reason as `annotations`: a fill, an
   * undo or a page move can change them.
   */
  readonly formFields: readonly FormField[];
  /**
   * Which repaint of the pages in place the history stands at: the newest
   * page-content step still applied (a watermark), 0 for the file as opened.
   * A page with the same id and turn shows the same pixels at the same
   * revision, so a viewer that keeps bitmaps by page adds it to its key: a
   * stamp, its undo and its redo paint the pages again, a page move or an
   * annotation does not. Absent from an engine that does not track it (read
   * it as 0).
   */
  readonly paintRevision?: number;
  /**
   * Which in-memory base the page-content items are read against: changes
   * when the core writes the pending content edits into a new base (a
   * checkpoint) or undo or redo steps across one. Every `PageContent` read
   * before the change is stale. Absent from an engine that does not track
   * it (read it as 0).
   */
  readonly contentBaseGeneration?: number;
}

/** A colour, each channel 0-255. */
export interface Rgb {
  readonly r: number;
  readonly g: number;
  readonly b: number;
}

/**
 * A box on a page as it is drawn, in PDF points from its top-left corner
 * with the page's `/Rotate` applied: the frame `search` and `selectText`
 * answer in, so a selection's rects can be marked up as they are.
 */
export interface PageRect {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

/** The text markup kinds: a band over, a line under, or a line through text. */
export type MarkupKind = 'highlight' | 'underline' | 'strikeout';

/**
 * One annotation edit, recorded as one undo step. Geometry is on the page as
 * drawn (see `PageRect`); `page` is a position as it is now and `id` an
 * `Annotation.id`.
 */
export type AnnotationEdit =
  /** One box of markup is one annotation: a selection over three lines is three edits. */
  | { readonly kind: 'addMarkup'; readonly page: number; readonly markup: MarkupKind; readonly rect: PageRect; readonly color: Rgb }
  /** A rectangle outline. */
  | { readonly kind: 'addShape'; readonly page: number; readonly rect: PageRect; readonly color: Rgb }
  /** One freehand stroke. */
  | { readonly kind: 'addInk'; readonly page: number; readonly points: readonly PagePoint[]; readonly color: Rgb }
  /** A sticky note. */
  | { readonly kind: 'addNote'; readonly page: number; readonly rect: PageRect; readonly contents: string }
  /** A visible text box; rejects with `EncodingGap` for text Helvetica cannot show. */
  | { readonly kind: 'addFreeText'; readonly page: number; readonly rect: PageRect; readonly contents: string }
  /**
   * An image stamp from PNG/JPEG bytes, copied to the engine (not
   * transferred: the caller keeps them to draw it). Rejects with
   * `InvalidImage` when they do not decode. Size it with `stampPlacement`.
   */
  | { readonly kind: 'addStamp'; readonly page: number; readonly rect: PageRect; readonly image: Uint8Array }
  /** Moves an annotation by (`dx`, `dy`) on the page as drawn. */
  | { readonly kind: 'moveAnnotation'; readonly id: number; readonly dx: number; readonly dy: number }
  /** Gives an annotation a new box (ink has none). */
  | { readonly kind: 'resizeAnnotation'; readonly id: number; readonly rect: PageRect }
  /** Recolours markup, a shape or ink. */
  | { readonly kind: 'recolorAnnotation'; readonly id: number; readonly color: Rgb }
  /** Retypes a free-text box; the same text records nothing. */
  | { readonly kind: 'retextAnnotation'; readonly id: number; readonly contents: string }
  | { readonly kind: 'removeAnnotation'; readonly id: number };

/**
 * What a form field is filled in with. A field's kind decides which one it
 * takes: `text` for a text field, `checked` for a checkbox, `choice` for a
 * radio group (an export value) or a dropdown; `null` chooses nothing.
 */
export type FieldValue =
  | { readonly kind: 'text'; readonly text: string }
  | { readonly kind: 'checked'; readonly checked: boolean }
  | { readonly kind: 'choice'; readonly option: string | null };

/**
 * Fills form field `fieldId` (a `FormField.id`) in, recorded as one undo
 * step. Unlike an annotation edit it changes the pixels (pdfium paints the
 * field), so the engine refreshes the preview inside the same request.
 * Rejects with `EditNotPermitted`, then `FormFieldNotFound`, then
 * `UnsupportedOperation` for a value the field cannot hold (the wrong kind, a
 * choice it does not offer, text past its `maxLength`). Filling a field with
 * the value it has is still a step, as on the native shells.
 */
export interface FormEdit {
  readonly kind: 'setFieldValue';
  readonly fieldId: number;
  readonly value: FieldValue;
}

/**
 * The text keys of the document's `/Info` (its properties, as the native
 * shells' Document properties dialog shows them). `null` is an absent key.
 */
export interface DocumentProperties {
  readonly title: string | null;
  readonly author: string | null;
  readonly subject: string | null;
  readonly keywords: string | null;
  readonly creator: string | null;
  readonly producer: string | null;
}

/**
 * Sets the document's properties as one undo step, written into `/Info` by
 * the next save (no XMP is touched, as on the native shells). A `null` or
 * empty key is removed; the creation and modification dates stay as they
 * are. The values the document already has record nothing. Rejects with
 * `EditNotPermitted` when the document withholds modification (see
 * `metadataRefusal`).
 */
export interface MetadataEdit {
  readonly kind: 'setDocumentProperties';
  readonly properties: DocumentProperties;
}

/**
 * What a watermark paints: a line of text in Helvetica (the core's grey;
 * `fontSize` in points), or a PNG or JPEG fitted, aspect kept, inside each
 * page's visible box shrunk by `scale` (`0` exclusive to `1`).
 */
export type WatermarkMark =
  | { readonly kind: 'text'; readonly text: string; readonly fontSize: number }
  | { readonly kind: 'picture'; readonly image: Uint8Array; readonly scale: number };

/**
 * Stamps the same mark on every page `pages` names (0-based positions), as
 * one undo step: page content, painted into each page's content stream by
 * the next save and shown by the pixels at once. The pages are resolved
 * once, so the stamp stays on them through a later reorder. `opacity` is
 * `0` exclusive to `1`, `rotationDegrees` counter-clockwise, `behind` paints
 * it under the page's content. Rejects with `EditNotPermitted` (or
 * `UnsupportedOperation`) as `watermarkRefusal` says, then
 * `PageIndexOutOfBounds`, `EncodingGap` for a character Helvetica cannot
 * show, `InvalidImage` for bytes that are not a picture, or
 * `UnsupportedOperation` for blank text, a repeated page or a look outside
 * the core's ranges.
 */
export interface WatermarkEdit {
  readonly kind: 'addWatermark';
  readonly pages: readonly number[];
  readonly mark: WatermarkMark;
  readonly opacity: number;
  readonly rotationDegrees: number;
  readonly behind: boolean;
}

/**
 * The looks the core starts every shell's first stamp from: a large grey
 * diagonal over the content for text, an upright half-page picture behind
 * it. Opacities and the scale are `0..1`.
 */
export interface WatermarkDefaults {
  readonly fontSize: number;
  readonly textOpacity: number;
  readonly textRotationDegrees: number;
  readonly textBehind: boolean;
  readonly pictureScale: number;
  readonly pictureOpacity: number;
  readonly pictureRotationDegrees: number;
  readonly pictureBehind: boolean;
}

/** Anything `applyEdit` records. */
export type Edit =
  | PageEdit
  | AnnotationEdit
  | FormEdit
  | MetadataEdit
  | WatermarkEdit
  | TextRunEdit
  | TextBlockEdit
  | ImageEdit
  | SelectionEdit;

/** One laid-out line of a free-text box. */
export interface FreeTextLine {
  readonly text: string;
  /** Left edge, from the box's left edge. */
  readonly xPt: number;
  /** Baseline, down from the box's top edge. */
  readonly baselineFromTopPt: number;
}

/**
 * How a free-text box's text lays out: the layout its saved appearance has.
 * Lines are placed in the box's own frame, before the page's turn: on a page
 * drawn turned, the text turns with it (by the page's `rotationDegrees`), as
 * it will in any PDF viewer.
 */
export interface FreeTextLayout {
  readonly fontSizePt: number;
  readonly lines: readonly FreeTextLine[];
  /** The lines run past the box's height; they are all there, clip them. */
  readonly overflow: boolean;
}

interface AnnotationBase {
  /** What annotation edits name it by. */
  readonly id: number;
  /** Position of its page now. */
  readonly pageIndex: number;
}

/** One annotation of this session, on the page as drawn. */
export type Annotation =
  | (AnnotationBase & { readonly kind: MarkupKind | 'shape'; readonly rect: PageRect; readonly color: Rgb })
  | (AnnotationBase & { readonly kind: 'ink'; readonly points: readonly PagePoint[]; readonly color: Rgb })
  | (AnnotationBase & { readonly kind: 'note'; readonly rect: PageRect; readonly contents: string })
  /** The engine keeps no image to hand back: the UI draws the one it added. */
  | (AnnotationBase & { readonly kind: 'stamp'; readonly rect: PageRect })
  | (AnnotationBase & {
      readonly kind: 'freeText';
      readonly rect: PageRect;
      readonly contents: string;
      readonly layout: FreeTextLayout;
    });

/**
 * One comment: a note the file carries (read-only, `annotationId` null) or
 * one this session added.
 */
export interface Comment {
  readonly pageIndex: number;
  readonly rect: PageRect;
  readonly contents: string;
  /** The file's `/T`; null for a session note. */
  readonly author: string | null;
  /** The file's own date string, unparsed; null for a session note. */
  readonly date: string | null;
  /** The session note's annotation id; null for a comment the file carries. */
  readonly annotationId: number | null;
}

interface FormFieldBase {
  /** What a `setFieldValue` edit names it by. */
  readonly id: number;
  /** Position of its page now. */
  readonly pageIndex: number;
  /** Its fully qualified `/T`. */
  readonly name: string;
  /** Its widget on the page as drawn (a radio group's whole group: see `buttons`). */
  readonly rect: PageRect;
  /**
   * The opened file already had it, so pdfium paints it (always, for now:
   * the web cannot add fields).
   */
  readonly existing: boolean;
}

/** One button of a radio group: the value it chooses, and where it is. */
export interface RadioButton {
  readonly exportValue: string;
  readonly rect: PageRect;
}

/**
 * One AcroForm field and what it holds now. Fields of other kinds (list
 * boxes, push buttons, signatures) stay in the file but are not listed.
 */
export type FormField =
  | (FormFieldBase & {
      readonly kind: 'text';
      readonly multiline: boolean;
      /** `/MaxLen` in characters; `null` when there is none. */
      readonly maxLength: number | null;
      readonly value: string;
    })
  | (FormFieldBase & { readonly kind: 'checkbox'; readonly checked: boolean })
  | (FormFieldBase & {
      readonly kind: 'radio';
      readonly buttons: readonly RadioButton[];
      /** The export value of the button on; `null` when none is. */
      readonly selected: string | null;
    })
  | (FormFieldBase & {
      /** A combo box. */
      readonly kind: 'dropdown';
      readonly options: readonly string[];
      /** Takes a value outside `options`. */
      readonly editable: boolean;
      readonly selected: string | null;
    });

/** A point on a page as it is drawn, in PDF points from its top-left corner. */
export interface PagePoint {
  readonly x: number;
  readonly y: number;
}

/**
 * Rects on a page as it is drawn, flattened: `left, top, width, height` per
 * rect, in PDF points from the page's top-left corner, with the page's
 * `/Rotate` already applied. The viewer only scales them by its zoom.
 */
export type PageRects = Float32Array;

/** One exact match of a search: its page and one rect per line it spans. */
export interface SearchHit {
  readonly pageIndex: number;
  readonly rects: PageRects;
}

/** Selected text on one page and the rects to paint it, one per line. */
export interface TextSelection {
  readonly text: string;
  readonly rects: PageRects;
}

/**
 * Names one open document in the engine. Every render says which document
 * it is for, so the engine can never paint a document the UI is not showing.
 */
export type DocumentHandle = number;

/** A freshly opened document: its handle and what the engine reports about it. */
export interface OpenedDocument {
  readonly handle: DocumentHandle;
  readonly info: DocumentInfo;
}

/** One rendered page: tightly packed RGBA, `width * height * 4` bytes. */
export interface RenderedPage {
  readonly width: number;
  readonly height: number;
  readonly rgba: Uint8ClampedArray;
}

/**
 * How hard a compression tries (the core's `CompressPreset`): `lossless`
 * repacks the file only, `balanced` also brings images down to 150 DPI,
 * `small` to 96 DPI. The numbers behind each are the core's.
 */
export type CompressPreset = 'lossless' | 'balanced' | 'small';

/** Why a compression could not do something, as the UI branches on it. */
export type CompressRefusalKind =
  /** The document is encrypted: no repack is possible while it stays protected. */
  | 'encryptedDocumentNotRewritable'
  /** The document is signed and the user was not asked. */
  | 'signaturesWouldBeInvalidated'
  /** A refusal this package does not model yet: show its `message`. */
  | 'other';

export interface CompressRefusal {
  readonly kind: CompressRefusalKind;
  /** The core's own words, lower-case and without a full stop. */
  readonly message: string;
}

/**
 * The two passwords of a new protection (the PDF's two roles): the one the
 * file asks for when it is opened, and the one that lifts its permissions.
 * Both are required and must differ — the core's rule.
 */
export interface ProtectionPasswords {
  readonly openPassword: string;
  readonly permissionsPassword: string;
}

/** A compressed save: the bytes and what compression did to them. */
export interface CompressedDocument {
  /** The PDF. Never larger than an ordinary save; exactly it when `reduced` is false. */
  readonly bytes: Uint8Array;
  /** Size of the ordinary save of the session (edits included), not of the opened file. */
  readonly beforeBytes: number;
  /** Size of `bytes`. */
  readonly afterBytes: number;
  /** `beforeBytes - afterBytes`, or zero. */
  readonly savedBytes: number;
  /** Whether `bytes` are strictly smaller than the ordinary save: when not, there is no smaller copy to offer. */
  readonly reduced: boolean;
  /** What could not be done, in the order the core found it. */
  readonly refusals: readonly CompressRefusal[];
}

/**
 * One part of a split: an inclusive run of zero-based page positions and the
 * file the core names it (`report-part1.pdf`, padded to the number of parts).
 */
export interface SplitPart {
  readonly first: number;
  readonly last: number;
  readonly fileName: string;
}

/** The encoding of an exported page image (the core's `ExportFormat`). JPEG has no alpha: the page is flattened onto white. */
export type ImageExportFormat = 'png' | 'jpeg';

/** An image export opened by `openImageExport`: a snapshot of the document to draw pages from. */
export type ImageExportHandle = number;

/** What `openImageExport` answers: the export, and how many pages its snapshot has. */
export interface ImageExport {
  readonly handle: ImageExportHandle;
  /**
   * The snapshot's page count: plan the export against it, not against a
   * page count read before the queued edits settled.
   */
  readonly pageCount: number;
}

/** What the UI may ask the engine to do. */
export interface Engine {
  /**
   * Opens `bytes` (with `password` for an encrypted document). Documents
   * already open stay open: the caller closes each one it no longer shows.
   */
  open(bytes: Uint8Array, password?: string): Promise<OpenedDocument>;
  /**
   * Renders a whole page of `document` at `dpi` (72 = one pixel per PDF
   * point). Rejects (`DocumentNotFound`) once that document was closed.
   */
  renderPage(document: DocumentHandle, pageIndex: number, dpi: number): Promise<RenderedPage>;
  /** Closes `document`; closing one that is not open does nothing. */
  close(document: DocumentHandle): Promise<void>;
  /**
   * The document with every edit applied, as a complete PDF that keeps the
   * document's own encryption. A document whose signatures the save would
   * break rejects with `SignaturesWouldBeInvalidated` unless
   * `invalidateSignatures` says the user was told. Rejects with `SaveFailed`
   * when the bytes cannot be written.
   */
  saveToBytes(document: DocumentHandle, invalidateSignatures: boolean): Promise<Uint8Array>;
  /**
   * Records one edit as one undo step and answers with the document as it is
   * now. Renders and text reads after it already show a page edit; an
   * annotation edit changes no pixels (the UI draws `annotations`); a fill
   * does, and renders after it already show it. Rejects with
   * `EditNotPermitted` (no assembly permission for a page edit, no annotate
   * permission for an annotation edit or a fill), `PageIndexOutOfBounds`,
   * `AnnotationNotFound`, `FormFieldNotFound`, `EncodingGap`, `InvalidImage`
   * or `UnsupportedOperation`, recording nothing.
   */
  applyEdit(document: DocumentHandle, edit: Edit): Promise<EditedDocument>;
  /** Takes back the last edit (nothing, when there is none). Same answer as `applyEdit`. */
  undo(document: DocumentHandle): Promise<EditedDocument>;
  /** Re-applies the last undone edit (nothing, when there is none). */
  redo(document: DocumentHandle): Promise<EditedDocument>;
  /**
   * Opens `bytes` as an import source (with `password` for an encrypted one)
   * and keeps it until `importPrepared` takes it or `discardImport` drops it.
   * Changes no document. Rejects with `PasswordRequired` or `WrongPassword`
   * when the source needs a password `password` is not (ask, then call
   * again), and `UnsupportedOperation` when it forbids copying its pages, has
   * none, or carries something an import cannot graft.
   */
  prepareImport(bytes: Uint8Array, password?: string): Promise<PreparedImport>;
  /** Drops a prepared source (an abandoned pick); dropping an unknown one does nothing. */
  discardImport(id: number): Promise<void>;
  /**
   * Adds every page of the prepared sources `ids`, in that order, at `index`
   * (the page count appends), as ONE undo step, and answers like `applyEdit`.
   * All or nothing: rejects with `EditNotPermitted`, `UnsupportedOperation`
   * or `PageIndexOutOfBounds` leaving every source prepared.
   */
  importPrepared(document: DocumentHandle, ids: readonly number[], index: number): Promise<ImportedDocument>;
  /**
   * Every exact, case-sensitive match of `query` (the core's search), in page
   * order. An empty query matches nothing. Rejects with `TextExtractionDenied`
   * when the document withholds its text.
   */
  search(document: DocumentHandle, query: string): Promise<SearchHit[]>;
  /**
   * The text between the characters nearest `anchor` and `focus` on one
   * page, in either order; empty when they meet or the page has no text.
   * Same refusal as `search`.
   */
  selectText(document: DocumentHandle, pageIndex: number, anchor: PagePoint, focus: PagePoint): Promise<TextSelection>;
  /** The word under `point`; empty on whitespace. Same refusal as `search`. */
  selectWord(document: DocumentHandle, pageIndex: number, point: PagePoint): Promise<TextSelection>;
  /** The session's annotations as they are now (what every `EditedDocument` carries). */
  annotations(document: DocumentHandle): Promise<Annotation[]>;
  /** Every comment, page by page: the file's own first, then the session's notes. Never refused for permissions. */
  comments(document: DocumentHandle): Promise<Comment[]>;
  /**
   * The document's form fields in `/Fields` order (the opened file's own
   * included, so not empty at open), each on the page as drawn: what every
   * `EditedDocument` carries. Never refused for permissions.
   */
  formFields(document: DocumentHandle): Promise<FormField[]>;
  /**
   * How `contents` lays out in a free-text box `widthPt` x `heightPt` (the
   * box's own frame): what the box will look like once added. Empty text is
   * fine; rejects with `EncodingGap` for a character Helvetica cannot show.
   */
  freeTextLayout(contents: string, widthPt: number, heightPt: number): Promise<FreeTextLayout>;
  /**
   * The box the core gives an image stamp dropped or pasted at `anchor` on a
   * page: the image's own proportions, its longest side two inches, the
   * image's own top-left corner at the anchor (on a page drawn turned, the
   * box's drawn corner turned with it, as in the native shells). Pass it to
   * an `addStamp` edit. The image
   * is copied, not transferred. Rejects with `InvalidImage`.
   */
  stampPlacement(document: DocumentHandle, pageIndex: number, image: Uint8Array, anchor: PagePoint): Promise<PageRect>;
  /** Every preset a compress dialog may offer, strongest-preserving first: the core's own list. */
  compressPresets(): Promise<CompressPreset[]>;
  /**
   * Why `document` cannot be compressed, or `null` when it can (today only
   * an encrypted document: the web keeps a document's protection on every
   * save). Ask before offering to run.
   */
  compressionRefusal(document: DocumentHandle): Promise<CompressRefusal | null>;
  /**
   * Whether compressing breaks a signature the file carries: `true` for a
   * signed file even with nothing edited (a compressed save is always a full
   * rewrite), where an ordinary save would not ask. When `true`,
   * `saveCompressedToBytes` rejects with `SignaturesWouldBeInvalidated`
   * unless `invalidateSignatures` says the user agreed.
   */
  compressionInvalidatesSignatures(document: DocumentHandle): Promise<boolean>;
  /**
   * The document with every edit applied, compressed as far as `preset`
   * allows. Records no edit. A refused compression is not a rejection: it
   * answers the ordinary save with `reduced` false and the reason in
   * `refusals`. Rejects with `SignaturesWouldBeInvalidated`, `SaveFailed`,
   * or `UnsupportedOperation` for a preset the core does not have.
   */
  saveCompressedToBytes(
    document: DocumentHandle,
    preset: CompressPreset,
    invalidateSignatures: boolean,
  ): Promise<CompressedDocument>;
  /**
   * The document's properties as the next save would write them: the last
   * pending change's, else the file's own. Answers however restricted the
   * document is.
   */
  documentProperties(document: DocumentHandle): Promise<DocumentProperties>;
  /**
   * Why `document`'s properties cannot be changed, as the core's lower-case
   * clause, or `null` when they can: its permissions withhold modification
   * (the gate the native shells ask). Ask before offering to edit.
   */
  metadataRefusal(document: DocumentHandle): Promise<string | null>;
  /**
   * Why an editor of `document`'s page content cannot be offered, as the
   * core's lower-case sentence, or `null` when it can: the content edit's own
   * gate (its permissions withhold modification, or its encryption cannot
   * survive the full rewrite a content edit forces), then the text
   * permission every content read is behind.
   */
  contentEditingRefusal(document: DocumentHandle): Promise<string | null>;
  /**
   * What page `pageIndex` paints — its text runs, images and vector paths —
   * as the reader sees it, every pending content edit laid on. Parsed on
   * every call (cache by page id, `paintRevision` and
   * `contentBaseGeneration`). Rejects with `TextExtractionDenied` or
   * `PageIndexOutOfBounds`.
   */
  readPageContent(document: DocumentHandle, pageIndex: number): Promise<PageContent>;
  /** The text blocks on a page, one line each. Same refusals as `readPageContent`. */
  readTextBlocks(document: DocumentHandle, pageIndex: number): Promise<TextBlock[]>;
  /** The text blocks on a page, stacked lines joined into paragraphs. Same refusals as `readPageContent`. */
  readParagraphBlocks(document: DocumentHandle, pageIndex: number): Promise<TextBlock[]>;
  /**
   * How run `runId` on a page looks, or `null` when the file cannot say (a
   * run only a pending insertion added). Rejects with `UnsupportedOperation`
   * for an id the page does not hold now, else as `readPageContent`.
   */
  textRunStyle(document: DocumentHandle, pageIndex: number, runId: number): Promise<TextRunStyle | null>;
  /** Each font a page declares, `/BaseFont` by resource name (raw). Same refusals as `readPageContent`. */
  pageFontFamilies(document: DocumentHandle, pageIndex: number): Promise<Record<string, string>>;
  /** The base every content read is valid against (see `EditedDocument.contentBaseGeneration`). */
  contentBaseGeneration(document: DocumentHandle): Promise<number>;
  /**
   * How run `runId` looks now — a queued restyle included — and which styles
   * can come off it: what bold / italic / underline toggles show. `null`
   * when the file cannot say. Same refusals as `textRunStyle`.
   */
  textRunLook(document: DocumentHandle, pageIndex: number, runId: number): Promise<TextRunLookState | null>;
  /**
   * The font program run `runId`'s font embeds, or `null` for one that
   * embeds none (a standard-14 font) or a run only a pending edit holds.
   * Same refusals as `textRunStyle`.
   */
  textRunFontProgram(document: DocumentHandle, pageIndex: number, runId: number): Promise<FontProgram | null>;
  /**
   * Whether image `imageId` on a page could be replaced now: ask before
   * opening a file picker. Rejects with `EditNotPermitted` when the
   * document forbids content changes, or `UnsupportedOperation` for an image
   * with an unsaved edit in a session that cannot take a checkpoint, an
   * image the page does not paint now, or a picture undo could not put back;
   * else as `readPageContent`.
   */
  checkImageReplacement(document: DocumentHandle, pageIndex: number, imageId: number): Promise<void>;
  /**
   * Copies `members` of page `pageIndex` into a clip, leaving the page as it
   * is (nothing to undo); members with an unsaved edit are copied as they
   * read now. With `forCut`, asks a cut's permission — text extraction and
   * content editing, `EditNotPermitted` otherwise — instead of the copy's
   * (`TextExtractionDenied`); the cut's removal is a `removeContentSelection`
   * edit that follows. Then `PageIndexOutOfBounds`, or
   * `UnsupportedOperation` for an empty selection or an item the page does
   * not hold now.
   */
  copyContentSelection(document: DocumentHandle, pageIndex: number, members: ContentMembers, forCut?: boolean): Promise<ContentClip>;
  /** The core's starting looks for a text and a picture watermark. */
  watermarkDefaults(): Promise<WatermarkDefaults>;
  /**
   * Why no watermark can be stamped on `document`, as the core's lower-case
   * clause, or `null` when one can: its permissions withhold modification,
   * or its encryption cannot survive the rewrite a stamp forces (the gate
   * the native shells ask). Ask before offering to stamp.
   */
  watermarkRefusal(document: DocumentHandle): Promise<string | null>;
  /**
   * Why `document` cannot be protected, as the core's lower-case clause, or
   * `null` when it can (today only a document whose permissions withhold
   * modification). Ask before offering to run.
   */
  protectionRefusal(document: DocumentHandle): Promise<string | null>;
  /**
   * Whether the protected copy breaks a signature the file carries. When
   * `true`, `protectToBytes` rejects with `SignaturesWouldBeInvalidated`
   * unless `invalidateSignatures` says the user agreed.
   */
  protectionInvalidatesSignatures(document: DocumentHandle): Promise<boolean>;
  /**
   * The document with every edit applied, written under new AES-128
   * protection with every permission granted (the bytes, transferred, not
   * copied). Records no edit, and the open document's own protection does
   * not change. Rejects with `InvalidPasswords` (one is missing, or both are
   * the same: the core's sentence), `EditNotPermitted` (see
   * `protectionRefusal`), `SignaturesWouldBeInvalidated` or `SaveFailed`.
   * The engine keeps neither password.
   */
  protectToBytes(
    document: DocumentHandle,
    passwords: ProtectionPasswords,
    invalidateSignatures: boolean,
  ): Promise<Uint8Array>;
  /** Every format an image export may write (`png`, `jpeg`): the core's own list. */
  imageExportFormats(): Promise<ImageExportFormat[]>;
  /**
   * Reads a one-based page selection (`"1-3,7"`) into ascending,
   * deduplicated zero-based positions — the core's grammar, the one every
   * shell asks. Rejects with `InvalidPageSelection`, whose message is the
   * core's sentence for the person who typed it.
   */
  parsePageSelection(input: string, totalPages: number): Promise<number[]>;
  /**
   * The file each of `pages` (zero-based) is exported under, named after the
   * document by the core: `report.pdf` page 3 of 12 as JPEG is
   * `report-03.jpg`. Always one path component.
   */
  pageImageFileNames(
    documentName: string,
    pages: readonly number[],
    totalPages: number,
    format: ImageExportFormat,
  ): Promise<string[]>;
  /**
   * The first of `pages` (zero-based, the document's current order) whose
   * whole page at `dpi` is over the raster ceiling every render enforces,
   * or `null` when all fit. Ask before an export starts, so the page is
   * named up front.
   */
  firstPageTooLargeToExport(document: DocumentHandle, pages: readonly number[], dpi: number): Promise<number | null>;
  /**
   * Starts an image export: a snapshot of what a save would write now (every
   * pending edit, annotation and fill in it; a signature is not asked about,
   * since nothing is written), which every page of the export is drawn from.
   * Records no edit. Rejects with `UnsupportedOperation` when the document
   * withholds extracting its pages. End it with `closeImageExport`.
   */
  openImageExport(document: DocumentHandle): Promise<ImageExport>;
  /**
   * One page of an export, rendered at `dpi` and encoded as `format` by the
   * core: the bytes of the image file (transferred, not copied). Rejects
   * with `DocumentNotFound` for an export that is not open,
   * `PageIndexOutOfBounds`, and `RenderFailed` for a page over the raster
   * ceiling.
   */
  exportPageImage(
    exportHandle: ImageExportHandle,
    pageIndex: number,
    dpi: number,
    format: ImageExportFormat,
  ): Promise<Uint8Array>;
  /** Ends an export, freeing its snapshot. `false` if it was not open. */
  closeImageExport(exportHandle: ImageExportHandle): Promise<boolean>;
  /**
   * Starts a print — **for printing only**, not a way to export: the same
   * snapshot as `openImageExport` (what a save would write now), drawn with
   * `exportPageImage` and ended with `closeImageExport`, but asked no
   * permission — no native shell asks one before printing, and the core has
   * no rule for the print bit. In exchange its pages are drawn at 150 DPI at
   * most: a finer `exportPageImage` on it is refused with
   * `UnsupportedOperation`, so a document that withholds extraction cannot
   * be drawn at export quality through it.
   */
  openPrintSnapshot(document: DocumentHandle): Promise<ImageExport>;
  /**
   * Why no page of `document` can be extracted (or split off), as the core's
   * sentence, or `null` when they can: its permissions withhold extracting
   * (`/P` bit 5, not page assembly: the open document does not change), or
   * its encryption cannot survive the rewrite a new page set needs (opened
   * with only one of its two passwords). Ask before offering to run.
   */
  extractionRefusal(document: DocumentHandle): Promise<string | null>;
  /** Whether the file as opened is signed: an extracted copy's signature no longer verifies. */
  extractSourceIsSigned(document: DocumentHandle): Promise<boolean>;
  /**
   * A new PDF of `pages` (zero-based, ascending and unique, in the
   * document's current order) with every pending edit applied, protected as
   * the source is (the bytes, transferred, not copied). Records no edit.
   * Rejects with `InvalidPageSelection` (empty, out of order),
   * `PageIndexOutOfBounds`, `UnsupportedOperation` (see
   * `extractionRefusal`) or `SaveFailed`.
   */
  extractPages(document: DocumentHandle, pages: readonly number[]): Promise<Uint8Array>;
  /**
   * The parts a split at `cuts` (one-based "split after page" positions,
   * `"3,7"`) makes of a document of `totalPages` named `documentName` — the
   * core's grammar and names, as on every shell. Rejects with
   * `InvalidPageSelection`, whose message is the core's sentence for the
   * person who typed it (one page, no cut, a cut after the last page).
   */
  planSplit(cuts: string, totalPages: number, documentName: string): Promise<SplitPart[]>;
}
