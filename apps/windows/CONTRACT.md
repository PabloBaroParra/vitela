# Windows Frontend Architecture Contract

**Status:** proposed contract for the first WinUI/C# implementation. This document freezes the Windows boundary without changing the Rust FFI surface. `pdf-ffi` is the source of truth for callable core operations; names labelled **proposed facade** are C#-only adapters.

## Boundary And Compatibility

```
WinUI views -> C# application facade -> generated pdf-ffi C# bindings -> Rust core
```

| Layer | Owns | Must not own |
|---|---|---|
| WinUI views | Presentation state, gestures, dialogs, visual composition | Document rules, encryption decisions, FFI handles, rendering scheduling |
| C# application facade | Session lifetime, DTO translation, background dispatch, stale-result suppression, user-safe errors, file byte I/O | PDF domain rules or reimplementation of Rust operations |
| Generated bindings | Mechanical UniFFI marshalling only | Handwritten business behavior |
| Rust core | Document, render, edit, save, security, typed FFI errors | WinUI or C# policy |

No view calls generated bindings directly. All binding calls pass through the facade, and all document/business decisions remain in Rust.

The generated C# bindings and native `pdf-ffi` library are one compatibility unit. Use `uniffi-bindgen-cs v0.11.0+v0.31.0` with `uniffi = "=0.31.0"`; regenerate bindings from the exact library shipped with the application. Update generator and UniFFI together, then validate the generated binding/library pair. Mismatches can fail at runtime with UniFFI contract version or checksum exceptions.

Evidence: [`core/pdf-ffi/Cargo.toml`](../../core/pdf-ffi/Cargo.toml), [`spikes/uniffi-cs/README.md`](../../spikes/uniffi-cs/README.md), [`docs/batches-b8-b13.md`](../../docs/batches-b8-b13.md).

## Facade Operations

These are **proposed facade methods**, not additions to `pdf-ffi`.

| Proposed facade operation | Calls current FFI | Facade responsibility |
|---|---|---|
| `Open(request)` | `open_from_bytes(bytes, password?)` | Read selected source into bytes, create a session, map typed errors. |
| `OpenWithPasswords(userPassword, ownerPassword)` | `open_with_passwords_from_bytes(bytes, userPassword, ownerPassword)` | Open the currently selected source with both roles; required before a protected full-rewrite save. Never retain or log passwords after the call. |
| `CreateBlank(request)` | `create_blank_document(pageSize, orientation)` | Create an empty session and publish empty UI state. |
| `GetSession(sessionId)` | `DocumentHandle.page_count()` | Return facade-owned session metadata, never the raw handle. |
| `RenderPage(request)` | `render_page`, bitmap metadata, `get_pixels` | Dispatch off the UI thread, materialize a UI bitmap DTO, discard stale results. |
| `ApplyEdit(request)` / `InsertImageStamp(request)` | `apply_edit` / `insert_image_stamp` | Translate validated UI commands to FFI DTOs; mark session render state stale. |
| `Undo(sessionId)` / `Redo(sessionId)` | `undo` / `redo` | Return whether state changed and refresh session metadata. |
| `Save(request)` | `save_to_bytes(intent)` | Write returned bytes through the selected destination; enforce protection consent flow. |
| `ReopenSaved(sessionId, bytes, credentials)` | `open_from_bytes` or `open_with_passwords_from_bytes` | Replace the session handle so rendering reflects saved edits. |

Path-based FFI helpers are not the Windows contract. The facade uses the bytes-based open/save operations, which are the canonical cross-platform entry points.

Evidence: [`core/pdf-ffi/src/document.rs`](../../core/pdf-ffi/src/document.rs), [`core/pdf-ffi/src/types.rs`](../../core/pdf-ffi/src/types.rs).

## Contract DTOs

The following are language-neutral shapes. Concrete C# records may add immutable UI-only metadata but must not leak generated binding objects.

```text
DocumentSource { displayName: string, bytes: binary }
OpenRequest { source: DocumentSource, password?: secret string }
CreateBlankRequest { pageSize: A4 | Letter | Custom(widthPt, heightPt), orientation: Portrait | Landscape }
Session { sessionId: opaque string, displayName: string, pageCount: uint, state: Empty | Ready | Dirty | Saving }
RenderRequest { sessionId: opaque string, pageIndex: uint, dpi: uint, invertContentColors: bool, sequence: uint64 }
RenderedPage { sessionId: opaque string, pageIndex: uint, sequence: uint64, width: uint, height: uint, stride: uint, rgba: binary }
EditRequest { sessionId: opaque string, command: EditCommand }
SaveRequest { sessionId: opaque string, destination: DocumentDestination, intent: Default | StripProtection, stripProtectionConsent: bool }
OperationResult<T> { value?: T, error?: UserSafeError }
```

`EditCommand` is limited to the currently exported FFI commands: page rotation/insertion/removal; highlight, underline, strikeout, shape, ink, text note; annotation removal. Coordinates are PDF points with a bottom-left origin. `RenderedPage.rgba` is RGBA8 row-major data; the facade releases binding bitmap references after materialization.

## Threading And Stale Results

All binding/core calls run off the UI thread. The facade alone provides cancellation semantics:

1. Before dispatch, coalesce duplicate or superseded render requests for the same session/page.
2. Assign a monotonically increasing sequence per `(sessionId, pageIndex)`.
3. On completion, publish only when the returned sequence equals that page's current sequence and the session remains current; otherwise discard pixels and result.

This is not Rust rendering cancellation. `pdf-ffi::render_page` submits with internal `Visible` priority and synchronously calls `.wait()`; its public FFI signature exposes no cancellation token, priority, or queue handle. Once it crosses FFI, the facade cannot abort or reprioritize it. The Rust render crate has internal job cancellation/priority facilities, but they are deliberately not ports in `pdf-ffi`.

Evidence: [`core/pdf-ffi/src/document.rs`](../../core/pdf-ffi/src/document.rs), [`core/pdf-render/src/actor.rs`](../../core/pdf-render/src/actor.rs), [`core/pdf-render/src/renderer.rs`](../../core/pdf-render/src/renderer.rs).

## Rendering And New Documents

`CreateBlank` produces a zero-page session. Rendering it returns `FfiError::DocumentNotFound`; the facade maps this specific case to `Session.state = Empty` and an ordinary empty-document UI, not an error dialog or technical log entry.

The render-side PDF is created only at open/create time. `ApplyEdit` and `InsertImageStamp` update the document model but do not update its render-side handle. To render saved edits, `Save` then `ReopenSaved` with the returned bytes; this is also required after adding the first page to a blank document.

Evidence: [`core/pdf-ffi/src/document.rs`](../../core/pdf-ffi/src/document.rs), [`core/pdf-ffi/src/error.rs`](../../core/pdf-ffi/src/error.rs).

## Save, Protection, And Reopen

Default save preserves encryption. A protected document that needs a full rewrite, including structural page edits, must have been opened with both user and owner passwords through `OpenWithPasswords(userPassword, ownerPassword)`; a single-password open is sufficient only where the core can use an incremental save. On a typed invalid-save response, the facade explains that both credentials are required and offers a protected reopen, never substitutes or invents credentials.

`StripProtection` requires an explicit, current user confirmation. The facade must refuse a save request without `stripProtectionConsent = true`; only then does it pass `StripProtection` to Rust, which records the consent audit event. The UI must state that the saved copy will be unprotected. After every successful save, reopen the saved bytes with the applicable credentials before rendering the saved state.

Evidence: [`core/pdf-ffi/src/document.rs`](../../core/pdf-ffi/src/document.rs), [`core/pdf-save/src/security.rs`](../../core/pdf-save/src/security.rs), [`core/pdf-ffi/tests/smoke.rs`](../../core/pdf-ffi/tests/smoke.rs).

## Errors And Logging

| FFI category | User-safe facade outcome |
|---|---|
| `PasswordRequired`, `WrongPassword` | Password prompt or retry; do not disclose which password role was wrong. |
| `UnsupportedSecurityHandler`, `UnsupportedOperation` | Explain that this document or action is not supported. |
| `DocumentNotFound` for a zero-page blank session | Empty document state. |
| `PageIndexOutOfBounds`, `AnnotationNotFound`, `BitmapNotFound` | Refresh/discard stale UI state; report only if the retry still fails. |
| `InvalidImage`, `InvalidSaveRequest` | Explain the failed user action and offer retry or a safe alternative. |
| `RenderFailed`, `Io` | Generic failure message with a correlation ID; log sanitized technical detail. |
| `Internal` or unexpected typed error | Generic failure message with a correlation ID. |

Log the typed category, operation name, correlation ID, session ID, page index, and sanitized exception detail for diagnosable failures. Never log passwords, document bytes, bitmap pixels, annotation text, or unredacted local paths. Never derive user-facing text by parsing raw exception strings.

Evidence: [`core/pdf-ffi/src/error.rs`](../../core/pdf-ffi/src/error.rs).

## Non-Goals And Required Ports

This contract does not add search/text extraction, thumbnail/prefetch scheduling, progress reporting, true rendering cancellation, or form editing. Advanced screens require explicit `pdf-ffi` ports before implementation:

| Screen capability | Required missing port |
|---|---|
| Search and text selection | Text-run/search query API with page coordinates and stale-result identity. |
| Thumbnail rail and prefetch | Render priority, bounded queue visibility, and cancellable request handle. |
| Long-operation progress | Progress callback or operation status port. |
| Real cancellation | FFI cancellation token/handle that reaches the render job before dequeue; mid-render cancellation remains a separate capability. |
| Forms | Form field discovery, value mutation, validation, appearance regeneration, and save integration. |

## First Facade Acceptance Criteria

- [ ] Views use only the C# facade; no generated binding type or Rust business rule appears in WinUI code.
- [ ] Generated bindings and native library are built and shipped from the same pinned UniFFI compatibility unit.
- [ ] Open, dual-password open, create blank, page count, render, supported edits, undo/redo, save, and reopen are implemented through the facade only.
- [ ] Every FFI call is off the UI thread; stale render completions cannot replace the current page image.
- [ ] A zero-page blank document displays the empty state when rendering returns `DocumentNotFound`.
- [ ] Protected structural saves prompt for both credentials; protection stripping requires explicit consent and is never implicit.
- [ ] User-visible failures use the taxonomy above, and diagnostic logs exclude secrets and document content.
- [ ] No advanced screen depends on an internal Rust renderer capability that lacks a `pdf-ffi` port.
