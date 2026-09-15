# ADR 0002: `Document` mints its own `PageId`s

- Status: Accepted
- Date: 2026-09-15
- Owners: Pablo Baro

## Context

Nothing in the core minted `PageId`s. `Command::InsertPage` and
`Command::ImportPages` both took `Page` values whose id the **caller** had
picked, so every shell had to know — and independently reimplement — a rule
that was written down nowhere: *an id, once minted, is spent, even if the page
wearing it has since been deleted.*

Getting it wrong was silent. `page_ids_are_unique` in `pdf-document` compares
incoming ids only against the pages **currently** in the document, so a
deleted page's id looks free while `save_backing.base` still owns it. The
command applied, the grid redrew, and the failure surfaced later and in
another crate: `pdf_save::bridge::replay_page_ops` re-derives
`PageId(0..base.page_count())` from the unchanged base on every save, found
the same id described as `Base` on one side and `Imported` on the other, and
refused the save — permanently, with a message that said nothing about where
the id came from.

This shipped and was reported from a real session (#143, fixed in `linux-gtk`
alone). Three things made it worth changing rather than documenting:

1. **The mistake is the obvious implementation.** "One past the highest id in
   the model" is what anyone writes first, and it is correct until the first
   delete.
2. **The failure is total.** The document stops being saveable and stays that
   way until the user undoes back past the insert.
3. **It was per-shell.** `linux-gtk` got it right in #143; `pdf-ffi` had always
   got it right with its own counter. Neither helped Windows, macOS or iOS,
   which would each have met it fresh — and, going by how long it took to find
   here, in a user's hands rather than in a test.

Two shells independently arriving at the same counter was the signal: the
model's invariant was being reimplemented outside the model.

## Decision

`pdf_document::Document` owns page identity.

- A private, monotonic `next_page_id` counter lives on `Document`, seeded by
  `Document::with_pages` — the one moment "highest id in the model" and
  "highest id ever minted" are the same number, because nothing has been
  deleted yet. `pdf_save::document_from_lopdf` is where every document is
  born, and it uses it.
- `Document::allocate_page_id` / `allocate_page_ids` mint; `Document::next_page_id`
  reads the cursor without spending it, for the one caller that cannot hold
  the document while it mints (a shell that opens source PDFs on a worker
  thread). Both report `None` rather than wrapping when the `PageId` space is
  exhausted, which is why the counter is a `u64` behind a `u32` id.
- The cursor is never allowed below one past the highest id the document
  currently holds. The floor can only move the cursor **forward**, so it is
  strictly safer than the counter alone; it exists so a page that entered
  `pages` without going through a command — a hand-built fixture, most often —
  cannot have its id handed out twice.
- `Command::apply` **claims** the ids of every command that puts pages into
  the document (`InsertPage`, `ImportPages`, `InsertPages`). A caller that
  minted a run off-thread therefore cannot leave the counter behind, whether
  or not it counted the run correctly.
- `Command::insert_page(index, page)` is gone. `Command::insert_blank_page(&mut
  Document, index, size, orientation)` replaces it: it takes no id, so there is
  no id for a caller to get wrong.

Ids are minted at **command construction**, not at apply: a command carries a
frozen id so redo re-inserts the same page the annotations and form fields on
it still name. Undo is not a refund — the base PDF a save replays against
still owns the id.

## Consequences and boundaries

- The three shells that have not yet met this (Windows, macOS, iOS) cannot:
  there is no longer an API that accepts a page id for a new page.
- `linux-gtk` loses `DocumentSession::next_page_id`, `document::next_page_id`,
  and the carry through `write::preview::edits` — the counter now rides across
  a preview refresh inside `document_model`, which is the nicer property.
  `pdf-ffi` loses `DocumentState::allocate_page_id` for the same reason.
- `Document` gains a private field, so it can no longer be built with struct
  literal syntax outside `pdf-document`. `Document::with_pages` is the
  replacement, and using it is what seeds the counter — a fixture that instead
  assigns `document.pages` is relying on the floor, not on the counter.
- Out of scope: `Document::pages` stays public and writable, so this constrains
  where *new* ids come from, not what can be put into the page list. Nothing
  here changes what any already-working save writes; page ids never reach the
  file, only the mapping used to build it.
