# ADR 0001: Existing PDF annotations are not re-read on open

- Status: Accepted
- Date: 2026-08-05
- Owners: Pablo Baro

## Context

`document_from_lopdf` (`core/pdf-save/src/bridge.rs`) always starts a freshly
opened document with `annotations: Default::default()`, by design — see its
own doc comment and the
`document_from_lopdf_starts_with_empty_annotations_and_edit_log` test.
`pdf-render`'s `PdfiumRenderer` never passes an annotation-rendering flag
either, so the rasterized page bitmap does not show PDF-native annotation
appearance streams. Neither gap is new or platform-specific.

The practical effect, confirmed live while testing Android annotation
parity: annotations added and saved through this app (or any other editor)
are genuinely written into the file's `/Annots` array — the save path has a
round-trip test proving the object is there after reload — but the moment
that file is reopened, they are invisible in the reader and absent from the
editable `AnnotationSet`. This happens identically on every shell (Linux,
Windows, macOS, iOS, Android), because all of them go through the same core
open path.

## Decision

Treat this as a known, out-of-scope gap rather than something to patch
platform-by-platform. The Android annotation-parity change does not attempt
to fix it. A future, separately scoped change should:

- Parse each page's existing `/Annots` entries into `pdf_document::Annotation`
  values during `document_from_lopdf` (or immediately after it), for the
  annotation kinds this app already understands.
- Pass an annotation-rendering flag through `pdf-render` so unedited,
  unsupported, or third-party annotations stay visible even before they are
  editable.

Until that lands, every shell's "reopen a saved file" flow should be assumed
to lose annotation editability and visibility, independent of which platform
wrote the file.

## Consequences and boundaries

- Keeps the Android annotation-parity change scoped to Android UI/FFI work
  instead of hiding a cross-platform core gap inside a platform diff.
- Cost: users keep their annotated PDF bytes, but lose the ability to see or
  edit those annotations again after a save/reopen cycle, on every shell,
  until the reread path exists.
- Out of scope here: the actual parsing and render-flag implementation, which
  touches `pdf-save`, `pdf-document`, and `pdf-render` and needs its own
  spec/design pass before implementation.
