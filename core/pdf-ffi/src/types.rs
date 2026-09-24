//! FFI-facing data types (T-040): UniFFI `Record`/`Enum` shapes for the
//! command surface, plus their conversions to/from the real
//! `pdf_document`/`pdf_render`/`pdf_save` types. Kept as thin, explicit
//! mapping structs/enums rather than re-exporting the core crates' own types
//! directly — those crates are not UniFFI-aware (by design, per each
//! crate's "never depends on uniffi" boundary), and a stable FFI shape must
//! not break every time an internal core type gains a field.

use pdf_document::{
    ContentItemId, DocumentInfo, FontKind, ImageItem, ImageSource, Orientation, PageContent,
    PageId, PageSize, PdfDate, PdfDateOffset, Rect, TextRun,
};

use crate::form::{FfiFieldValue, FfiRadioOption, FfiTextStyle};

/// Mirrors `pdf_document::PageSize`.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum FfiPageSize {
    A4,
    Letter,
    Custom { width_pt: f64, height_pt: f64 },
}

impl From<FfiPageSize> for PageSize {
    fn from(size: FfiPageSize) -> Self {
        match size {
            FfiPageSize::A4 => PageSize::A4,
            FfiPageSize::Letter => PageSize::Letter,
            FfiPageSize::Custom {
                width_pt,
                height_pt,
            } => PageSize::Custom {
                width_pt,
                height_pt,
            },
        }
    }
}

/// Mirrors `pdf_document::Orientation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiOrientation {
    Portrait,
    Landscape,
}

impl From<FfiOrientation> for Orientation {
    fn from(orientation: FfiOrientation) -> Self {
        match orientation {
            FfiOrientation::Portrait => Orientation::Portrait,
            FfiOrientation::Landscape => Orientation::Landscape,
        }
    }
}

/// Mirrors `pdf_document::Rect` (page-space rectangle, points, origin
/// bottom-left) — distinct from `pdf_render::Rect` (a render-time clip
/// region in the same units but a different field shape); this FFI type
/// only ever crosses at the `pdf_document`/`pdf_annotate` annotation-rect
/// meaning.
/// One page's layout size in PDF points (`/Rotate`-aware — 90/270 swap the
/// axes), read from the same bytes `render_page` draws so viewers can size
/// placeholders that match the rendered output.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiPageDimensions {
    pub width_pt: f64,
    pub height_pt: f64,
}

impl From<pdf_manip::PageDimensions> for FfiPageDimensions {
    fn from(dimensions: pdf_manip::PageDimensions) -> Self {
        FfiPageDimensions {
            width_pt: dimensions.width_pt,
            height_pt: dimensions.height_pt,
        }
    }
}

/// A page-space rectangle in PDF points with a bottom-left origin, suitable
/// for mapping directly onto a rendered page.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiTextRect {
    pub x_pt: f64,
    pub y_pt: f64,
    pub width_pt: f64,
    pub height_pt: f64,
}

impl From<pdf_render::TextRect> for FfiTextRect {
    fn from(rect: pdf_render::TextRect) -> Self {
        Self {
            x_pt: f64::from(rect.x_pt),
            y_pt: f64::from(rect.y_pt),
            width_pt: f64::from(rect.width_pt),
            height_pt: f64::from(rect.height_pt),
        }
    }
}

/// Extracted text from one font run, with one PDF-space rectangle per Unicode
/// scalar in `text`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiTextRun {
    pub text: String,
    pub character_bounds: Vec<FfiTextRect>,
}

impl From<pdf_render::TextRun> for FfiTextRun {
    fn from(run: pdf_render::TextRun) -> Self {
        Self {
            text: run.text,
            character_bounds: run.character_bounds.into_iter().map(Into::into).collect(),
        }
    }
}

/// One exact-text match with its 0-indexed page and per-character PDF-space
/// rectangles. Empty queries return no matches.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiSearchResult {
    pub page_index: u32,
    pub text: String,
    pub character_bounds: Vec<FfiTextRect>,
}

impl From<pdf_render::TextMatch> for FfiSearchResult {
    fn from(found: pdf_render::TextMatch) -> Self {
        Self {
            page_index: found.page_index,
            text: found.text,
            character_bounds: found.character_bounds.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<FfiRect> for Rect {
    fn from(rect: FfiRect) -> Self {
        Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl From<Rect> for FfiRect {
    fn from(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

/// Mirrors `pdf_document::Color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FfiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<FfiColor> for pdf_document::Color {
    fn from(color: FfiColor) -> Self {
        pdf_document::Color {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

impl From<pdf_document::Color> for FfiColor {
    fn from(color: pdf_document::Color) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

/// A single point of an ink/freehand stroke (mirrors the `(f64, f64)` tuple
/// `pdf_document::AnnotationKind::Ink` stores — UniFFI records need named
/// fields, tuples don't cross the boundary directly).
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiPoint {
    pub x: f64,
    pub y: f64,
}

/// A persisted annotation exposed to shells for hit testing and local preview.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiAnnotation {
    pub id: u64,
    pub page: u32,
    pub kind: FfiAnnotationKind,
}

/// The editable annotation shapes supported by the cross-platform shell API.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FfiAnnotationKind {
    Highlight {
        rect: FfiRect,
        color: FfiColor,
    },
    Underline {
        rect: FfiRect,
        color: FfiColor,
    },
    Strikeout {
        rect: FfiRect,
        color: FfiColor,
    },
    Ink {
        points: Vec<FfiPoint>,
        color: FfiColor,
    },
    Shape {
        rect: FfiRect,
        color: FfiColor,
    },
    TextNote {
        rect: FfiRect,
        contents: String,
    },
    Stamp {
        rect: FfiRect,
    },
}

/// Mirrors `pdf_render::RenderOptions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Record)]
pub struct FfiRenderOptions {
    pub invert_content_colors: bool,
}

/// A bounded page tile in the output pixel coordinate space at the requested DPI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FfiRenderTile {
    pub left_px: u32,
    pub top_px: u32,
    pub width_px: u32,
    pub height_px: u32,
}

impl From<FfiRenderTile> for pdf_render::Tile {
    fn from(tile: FfiRenderTile) -> Self {
        Self {
            left: tile.left_px,
            top: tile.top_px,
            width: tile.width_px,
            height: tile.height_px,
        }
    }
}

impl From<FfiRenderOptions> for pdf_render::RenderOptions {
    fn from(options: FfiRenderOptions) -> Self {
        pdf_render::RenderOptions {
            invert_content_colors: options.invert_content_colors,
        }
    }
}

/// Mirrors `pdf_save::SaveIntent`: the caller's declared intent for how
/// `save`/`save_to_bytes`/`save_to_path` should treat an existing
/// `SecurityContext` (spec "Encrypted Document Save Behavior"). Choosing
/// `StripProtection` from a shell UI is exactly the "explicit, user-consented
/// removal of protection" the spec requires — see `document::save_to_bytes`,
/// which records the audit-log consent event when this variant is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum FfiSaveIntent {
    #[default]
    Default,
    StripProtection,
}

/// The permission mask used when a shell applies password protection without
/// offering a restrictions editor. The passwords control access; every
/// document permission remains granted.
pub(crate) const GRANTED_PROTECTION_PERMISSIONS: u32 = 0xFFFF_FFFC;

impl From<FfiSaveIntent> for pdf_save::SaveIntent {
    fn from(intent: FfiSaveIntent) -> Self {
        match intent {
            FfiSaveIntent::Default => pdf_save::SaveIntent::Default,
            FfiSaveIntent::StripProtection => pdf_save::SaveIntent::StripProtection,
        }
    }
}

/// Whether the shell has told the user that this save will break a signature
/// the file already carries — the FFI shape of
/// `pdf_save::SignatureAcknowledgement`.
///
/// A rewrite cannot preserve a signature, so there is nothing to *offer* the
/// user except the choice. `Unacknowledged` is the default precisely so that
/// a shell which has not built that choice yet fails loudly instead of
/// destroying a signature quietly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, uniffi::Enum)]
pub enum FfiSignatureAcknowledgement {
    #[default]
    Unacknowledged,
    ProceedAndInvalidate,
}

impl From<FfiSignatureAcknowledgement> for pdf_save::SignatureAcknowledgement {
    fn from(acknowledgement: FfiSignatureAcknowledgement) -> Self {
        match acknowledgement {
            FfiSignatureAcknowledgement::Unacknowledged => {
                pdf_save::SignatureAcknowledgement::Unacknowledged
            }
            FfiSignatureAcknowledgement::ProceedAndInvalidate => {
                pdf_save::SignatureAcknowledgement::ProceedAndInvalidate
            }
        }
    }
}

/// Mirrors `pdf_document::FontKind` (T-158): which font machinery a page
/// content text run's active font uses, which decides whether replacement
/// text can be encoded at all (Batch 21 decision 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiFontKind {
    Standard14,
    EmbeddedSimple,
    EmbeddedComposite,
}

impl From<FontKind> for FfiFontKind {
    fn from(kind: FontKind) -> Self {
        match kind {
            FontKind::Standard14 => FfiFontKind::Standard14,
            FontKind::EmbeddedSimple => FfiFontKind::EmbeddedSimple,
            FontKind::EmbeddedComposite => FfiFontKind::EmbeddedComposite,
            // `FontKind` is `#[non_exhaustive]` from this crate's
            // perspective; a future variant is conservatively treated as
            // not editable rather than failing to compile.
            _ => FfiFontKind::EmbeddedComposite,
        }
    }
}

impl From<FfiFontKind> for FontKind {
    fn from(kind: FfiFontKind) -> Self {
        match kind {
            FfiFontKind::Standard14 => FontKind::Standard14,
            FfiFontKind::EmbeddedSimple => FontKind::EmbeddedSimple,
            FfiFontKind::EmbeddedComposite => FontKind::EmbeddedComposite,
        }
    }
}

/// A text run parsed from a page's content stream (T-158) — distinct from
/// [`FfiTextRun`], which is pdfium's *extracted* text used for search/copy.
/// This one is `pdf-edit`'s model of an editable `Tj`/`TJ` operator, keyed by
/// [`ContentItemId`] rather than a render-side index.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiContentTextRun {
    pub id: u64,
    pub page: u32,
    pub bbox: FfiRect,
    pub resource_font_name: String,
    pub font_kind: FfiFontKind,
    pub text: String,
}

impl From<TextRun> for FfiContentTextRun {
    fn from(run: TextRun) -> Self {
        Self {
            id: run.id.0,
            page: run.page.0,
            bbox: run.bbox.into(),
            resource_font_name: run.resource_font_name,
            font_kind: run.font_kind.into(),
            text: run.text,
        }
    }
}

impl From<FfiContentTextRun> for TextRun {
    fn from(run: FfiContentTextRun) -> Self {
        Self {
            id: ContentItemId(run.id),
            page: PageId(run.page),
            bbox: run.bbox.into(),
            resource_font_name: run.resource_font_name,
            font_kind: run.font_kind.into(),
            text: run.text,
        }
    }
}

/// An image parsed from a page's content stream (T-158). Bytes are
/// deliberately absent, matching `pdf_document::ImageItem` — only
/// `Command::ReplaceImageSource`/`InsertImage` carry them, explicitly.
///
/// `pdf_document::ImageSource` arrives here as an **optional name**: absent
/// means the image is inline, its samples sitting in the content stream with
/// no resource to name. The two carry exactly the same information — the
/// format has two ways to paint an image and no more — and a nullable string
/// is what every binding this crate generates already speaks, where an enum
/// with a payload becomes a class hierarchy in three languages for one bit.
/// The round trip below is lossless in both directions, which is the
/// property that matters.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiContentImageItem {
    pub id: u64,
    pub page: u32,
    pub bbox: FfiRect,
    pub resource_xobject_name: Option<String>,
}

impl From<ImageItem> for FfiContentImageItem {
    fn from(item: ImageItem) -> Self {
        Self {
            id: item.id.0,
            page: item.page.0,
            bbox: item.bbox.into(),
            resource_xobject_name: match item.source {
                ImageSource::Resource(name) => Some(name),
                ImageSource::Inline => None,
            },
        }
    }
}

impl From<FfiContentImageItem> for ImageItem {
    fn from(item: FfiContentImageItem) -> Self {
        Self {
            id: ContentItemId(item.id),
            page: PageId(item.page),
            bbox: item.bbox.into(),
            source: match item.resource_xobject_name {
                Some(name) => ImageSource::Resource(name),
                None => ImageSource::Inline,
            },
        }
    }
}

/// The parsed content of one page — the FFI shape of
/// `pdf_document::PageContent`, returned by
/// `DocumentHandle::read_page_content` (T-158). Ids are only valid against
/// the exact bytes they were parsed from; re-read after a save before
/// building a second content edit for the same page (Batch 21 decision 6).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiPageContent {
    pub text_runs: Vec<FfiContentTextRun>,
    pub images: Vec<FfiContentImageItem>,
}

impl From<PageContent> for FfiPageContent {
    fn from(content: PageContent) -> Self {
        Self {
            text_runs: content.text_runs.into_iter().map(Into::into).collect(),
            images: content.images.into_iter().map(Into::into).collect(),
        }
    }
}

/// Mirrors `pdf_document::PdfDateOffset` (T-173, Batch 22): the relationship
/// of a [`FfiPdfDate`]'s local time to UT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiPdfDateOffset {
    Utc,
    Plus { hours: u8, minutes: u8 },
    Minus { hours: u8, minutes: u8 },
}

impl From<PdfDateOffset> for FfiPdfDateOffset {
    fn from(offset: PdfDateOffset) -> Self {
        match offset {
            PdfDateOffset::Utc => FfiPdfDateOffset::Utc,
            PdfDateOffset::Plus { hours, minutes } => FfiPdfDateOffset::Plus { hours, minutes },
            PdfDateOffset::Minus { hours, minutes } => FfiPdfDateOffset::Minus { hours, minutes },
        }
    }
}

impl From<FfiPdfDateOffset> for PdfDateOffset {
    fn from(offset: FfiPdfDateOffset) -> Self {
        match offset {
            FfiPdfDateOffset::Utc => PdfDateOffset::Utc,
            FfiPdfDateOffset::Plus { hours, minutes } => PdfDateOffset::Plus { hours, minutes },
            FfiPdfDateOffset::Minus { hours, minutes } => PdfDateOffset::Minus { hours, minutes },
        }
    }
}

/// Mirrors `pdf_document::PdfDate` (T-173, Batch 22) — a parsed
/// `/CreationDate`/`/ModDate` value, not a raw PDF date string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct FfiPdfDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub offset: FfiPdfDateOffset,
}

impl From<PdfDate> for FfiPdfDate {
    fn from(date: PdfDate) -> Self {
        Self {
            year: date.year,
            month: date.month,
            day: date.day,
            hour: date.hour,
            minute: date.minute,
            second: date.second,
            offset: date.offset.into(),
        }
    }
}

impl From<FfiPdfDate> for PdfDate {
    fn from(date: FfiPdfDate) -> Self {
        Self {
            year: date.year,
            month: date.month,
            day: date.day,
            hour: date.hour,
            minute: date.minute,
            second: date.second,
            offset: date.offset.into(),
        }
    }
}

/// Mirrors `pdf_document::DocumentInfo` (T-167/T-173, Batch 22) — the
/// `/Info` dict's seven standard text keys plus its two dates. `None` means
/// the key is absent from `/Info`, never "present with an empty string"
/// (batch decision 3) — a shell that clears a field must send `None`, not
/// `Some(String::new())`, unless it deliberately wants that distinct state.
#[derive(Debug, Clone, PartialEq, Eq, Default, uniffi::Record)]
pub struct FfiDocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<FfiPdfDate>,
    pub mod_date: Option<FfiPdfDate>,
}

impl From<DocumentInfo> for FfiDocumentInfo {
    fn from(info: DocumentInfo) -> Self {
        Self {
            title: info.title,
            author: info.author,
            subject: info.subject,
            keywords: info.keywords,
            creator: info.creator,
            producer: info.producer,
            creation_date: info.creation_date.map(Into::into),
            mod_date: info.mod_date.map(Into::into),
        }
    }
}

impl From<FfiDocumentInfo> for DocumentInfo {
    fn from(info: FfiDocumentInfo) -> Self {
        Self {
            title: info.title,
            author: info.author,
            subject: info.subject,
            keywords: info.keywords,
            creator: info.creator,
            producer: info.producer,
            creation_date: info.creation_date.map(Into::into),
            mod_date: info.mod_date.map(Into::into),
        }
    }
}

/// The FFI-facing shape of `pdf_document::Command` (T-040's `apply_edit`
/// surface). One variant per real `Command`/annotation-kind combination
/// this workspace supports as of Batch 7 — `move`/`resize`/`restyle` are
/// deliberately absent (documented Batch 5 gap: not yet `EditLog` commands,
/// see `pdf-annotate::ops` module docs).
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum FfiEditCommand {
    RotatePage {
        page: u32,
        delta_degrees: i32,
    },
    InsertBlankPage {
        index: u32,
        size: FfiPageSize,
        orientation: FfiOrientation,
    },
    RemovePage {
        index: u32,
    },
    AddHighlight {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddUnderline {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddStrikeout {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddShape {
        page: u32,
        rect: FfiRect,
        color: FfiColor,
    },
    AddInk {
        page: u32,
        points: Vec<FfiPoint>,
        color: FfiColor,
    },
    AddTextNote {
        page: u32,
        rect: FfiRect,
        contents: String,
    },
    RemoveAnnotation {
        annotation_id: u64,
    },
    MoveAnnotation {
        annotation_id: u64,
        dx: f64,
        dy: f64,
    },
    ResizeAnnotation {
        annotation_id: u64,
        rect: FfiRect,
    },
    RestyleAnnotation {
        annotation_id: u64,
        color: FfiColor,
    },

    // --- Page content (Batch 21, T-158) --------------------------------
    //
    // Mirror `pdf_document::Command`'s ten page-content variants. Each
    // carries the item read from `DocumentHandle::read_page_content` — no
    // id is allocated here, unlike annotations, because the item already
    // carries the identity the parser assigned it.
    ReplaceTextRunContent {
        item: FfiContentTextRun,
        after: String,
    },
    ReplaceTextRunWithInsertedFont {
        item: FfiContentTextRun,
        after: String,
    },
    InsertTextRun {
        item: FfiContentTextRun,
    },
    RemoveTextRun {
        item: FfiContentTextRun,
    },
    MoveTextRun {
        item: FfiContentTextRun,
        to: FfiRect,
    },
    InsertImage {
        item: FfiContentImageItem,
        source: Option<Vec<u8>>,
    },
    RemoveImage {
        item: FfiContentImageItem,
        source: Option<Vec<u8>>,
    },
    MoveImage {
        item: FfiContentImageItem,
        to: FfiRect,
    },
    ResizeImage {
        item: FfiContentImageItem,
        to: FfiRect,
    },
    ReplaceImageSource {
        item: FfiContentImageItem,
        before: Vec<u8>,
        after: Vec<u8>,
    },

    // --- Form fields (Batch 20, T-140) ----------------------------------
    //
    // The four `Add*` variants carry no id and no name: both are allocated
    // by `form::add_field` against the open document, because both have to
    // be unique across fields this session never created (see that module's
    // header). Every other variant names its field by `id` and carries only
    // the attribute it changes — the `from` half of the real
    // `MoveFormField`/`RestyleFormField`/`SetFieldValue`/`RenameFormField`
    // is resolved from the current `FormFieldSet`, the same pattern
    // `RemoveAnnotation` and `SetDocumentInfo` already use here.
    AddTextField {
        page: u32,
        rect: FfiRect,
        style: FfiTextStyle,
        multiline: bool,
        max_len: Option<u32>,
    },
    AddCheckbox {
        page: u32,
        rect: FfiRect,
        style: FfiTextStyle,
    },
    AddRadioGroup {
        page: u32,
        rect: FfiRect,
        style: FfiTextStyle,
        options: Vec<FfiRadioOption>,
    },
    AddDropdown {
        page: u32,
        rect: FfiRect,
        style: FfiTextStyle,
        options: Vec<String>,
        editable: bool,
    },
    RemoveFormField {
        field_id: u64,
    },
    /// Repositions a field. `MoveFormField` and `ResizeFormField` carry the
    /// same payload and mean different user intents — a drag versus a
    /// handle — exactly as the core commands they translate to do.
    MoveFormField {
        field_id: u64,
        to: FfiRect,
    },
    ResizeFormField {
        field_id: u64,
        to: FfiRect,
    },
    RestyleFormField {
        field_id: u64,
        style: FfiTextStyle,
    },
    /// Sets what the field holds — the fill half of the surface, and the one
    /// field command a document can permit on its own (see
    /// `form::is_structural_form_command`). Validated against the field's
    /// kind before it is recorded.
    SetFieldValue {
        field_id: u64,
        value: FfiFieldValue,
    },
    /// Renames a field's `/T`. Not in T-140's own list, and added anyway:
    /// the core command exists, `pdf_form::rename_field` validates it, and
    /// leaving it out would make this boundary the only place a field can be
    /// created but never named — the GTK4 shell's inspector offers it.
    RenameFormField {
        field_id: u64,
        name: String,
    },

    // --- Document metadata (Batch 22, T-173) ----------------------------
    //
    // `before` is not part of this variant: `DocumentState::build_core_command`
    // resolves it itself, from the last pending `SetDocumentInfo` if one is
    // already queued (decision 5's "last one wins") or from the file's
    // current `/Info` otherwise — a caller only ever states the value it
    // wants next.
    SetDocumentInfo {
        after: FfiDocumentInfo,
    },
}
