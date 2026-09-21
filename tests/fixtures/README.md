# tests/fixtures

Test corpora shared across the workspace, per `design.md`'s
`tests/fixtures/` layout.

## encrypted/ — encrypted-PDF corpus (T-004)

Two committed, statically-generated fixtures covering the standard security
handlers referenced by spec.md's "Open Password-Protected PDF" requirement:

| File | Algorithm | Handler | User password | Owner password |
|---|---|---|---|---|
| `rc4_128_user_and_owner.pdf` | RC4 | `/V 2 /R 3`, 128-bit key | `user-rc4-pass` | `owner-rc4-pass` |
| `aes_128_user_and_owner.pdf` | AES-128 | `/V 4 /R 4`, crypt filter `AESV2` | `user-aes-pass` | `owner-aes-pass` |

Each fixture uses **distinct, non-empty** user and owner passwords so a
single file exercises both the "correct user password" and "correct owner
password" open scenarios, plus the "wrong password" error scenario, from a
single corpus entry.

Regenerate with:

```sh
cargo run -p gen-fixtures
```

See `tests/fixtures/gen-fixtures/` for the generator source and its
integration tests (`generate_corpus.rs`), which verify every fixture is
detected as encrypted, opens with either correct password via
`lopdf::LoadOptions::with_password`, and cleanly rejects a wrong password.

Consumed by `pdf-manip`'s decrypt-on-open integration tests (Batch 4,
T-025/T-026) and any shell-level password-prompt testing (Batch 8+).

## signed/ — known-good signed-PDF corpus (T-078)

Statically-generated fixtures signed through the REAL production pipeline
(pdf-save incremental hook → `pdf_sign::digest_byte_ranges` →
`CmsSignedDataBuilder` → `PfxCertificateSource`), using rcgen self-signed
identities. **Test use only** — the signer certificates are self-signed and
carry no trust.

| File | Key | Signature scheme | Digest | Signatures |
|---|---|---|---|---|
| `rsa2048_sha256.pdf` | RSA-2048 | RSASSA-PKCS1-v1_5 | SHA-256 | 1 |
| `p256_sha256.pdf` | ECDSA P-256 | ECDSA | SHA-256 | 1 |
| `p256_sha384.pdf` | ECDSA P-256 | ECDSA | SHA-384 | 1 |
| `two_signatures_rsa2048_sha256.pdf` | RSA-2048 | RSASSA-PKCS1-v1_5 | SHA-256 | 2 |

Every signature is `adbe.pkcs7.detached` with a `/ByteRange` covering its
complete revision. The two-signature fixture proves the spec.md acceptance
criterion "a second signature must not invalidate the first": each signature
verifies independently over its own byte ranges.

Regenerate with:

```sh
cargo run -p gen-fixtures
```

**Regeneration is not byte-reproducible**: each run mints fresh random keys
and certificates, so the regenerated files always differ from the committed
ones. The known-good property is guaranteed by the generator's integration
tests (`signed_corpus.rs`), not by byte equality: on every test run they
regenerate a corpus, re-derive each `/ByteRange` digest, compare it against
the CMS `message-digest` signed attribute, verify the certificate's own
self-signature, cryptographically verify each CMS signature, and assert the
fields are discoverable through `/AcroForm /Fields` and the page's
`/Annots` (PDF 32000-1 §12.7.2).

Consumed by structural cross-validation (T-079) and the signing test suite
(T-080).

### External validator checklist (T-079)

Run this before a release against each file in `tests/fixtures/signed/` and a
representative production-signed PDF:

1. Open the PDF in Adobe Acrobat Reader and confirm each signature is listed.
2. Inspect each signature's properties and confirm the signed revision has not
   been modified; self-signed fixture certificates are expected to be untrusted.
3. Open the same file in an independent validator such as `pdfsig` from
   Poppler and confirm each `/ByteRange` and detached CMS signature is valid.
4. For `two_signatures_rsa2048_sha256.pdf`, confirm both signatures validate
   independently and the first covers an earlier revision.

The CI structural check is `cargo test -p pdf-sign --test
signed_fixture_validation`; it validates `/ByteRange` boundaries and compares
each recomputed document digest to the CMS `message-digest` signed attribute.

## forms/ — AcroForm interop fixture (T-144)

One committed fixture, and it is committed precisely because **this
workspace did not write it**.

| File | Produced by | Size |
|---|---|---|
| `reportlab_acroform.pdf` | reportlab, once, via the script beside it | ~12 KB |

Every other AcroForm this repository parses is built with lopdf — the same
library `pdf-form`'s writer half uses — so those tests prove the reader
agrees with the writer and nothing more. ISO 32000-1 leaves most of a field
tree's layout free, and producers use that freedom differently.

It earned its keep on the first run. reportlab restates the *inheritable*
`/FT` on every radio-button widget, which `pdf_form::read::kids_are_widgets`
read as "this kid is a field of its own" — turning one radio group into one
nameless checkbox per button, both answering to the parent's `/T`. Every
lopdf-built fixture in the repository omits `/FT` on its kids, so the entire
suite agreed with itself straight past it.

The page carries one field of each kind `pdf-form` models — text, multiline
text, checkbox, radio group, dropdown — plus a **listbox**, which it
deliberately does not: T-137's read resilience says an unmodeled field stays
intact in the file and simply never appears in the editable set, and a
fixture containing only readable fields could never catch a regression that
quietly started modelling one.

Regenerate with:

```sh
pip install reportlab
python tests/fixtures/forms/generate_reportlab_acroform.py
```

The script is not run by CI or by any test — it documents how the file was
made and lets it be rebuilt if lost, the same "external tool, generated once,
versioned" criterion `content-edit/generate_reportlab_embedded_subset.py`
uses. Regenerating is **not** expected as part of ordinary work: the
committed bytes are the fixture.

Consumed by `pdf-form`'s `external_acroform.rs` (the read path),
`pdf-save`'s `forms_roundtrip.rs` (fill a foreign form, save incrementally,
original bytes intact as prefix) and, through the second of those, the
`AcroForm round-trip validation (T-146)` CI job, which re-reads the result
with pypdf.

A **generated** AcroForm lives beside it in code rather than on disk —
`gen_fixtures::forms::build_acroform_document` — for tests that need to
dictate the starting state exactly instead of describing whatever reportlab
happened to emit.

## compress/ — compression corpus (T-197)

Five committed fixtures, generated by `gen-fixtures`, that exist because the
rest of this repository could not exercise `pdf-compress`. T-193 measured
every image in every real PDF here and found the most detailed one at **37
effective dpi** — well under the lowest preset's 96 — so the resampler could
run over the whole corpus, find no candidate, and pass.

| File | What it is there to catch | Size |
|---|---|---|
| `scan_200dpi.pdf` | a lossy image far above every preset's ceiling: 1700 x 2200 `/DCTDecode` samples over US Letter | ~238 KB |
| `reused_image_two_scales.pdf` | *the largest placement governs* — one XObject painted 72pt and 18pt across, so 300 dpi and 1200 dpi | ~88 KB |
| `transparency_smask.pdf` | real transparency: a `/DCTDecode` photograph with a `/FlateDecode` `/SMask`, both at 300 dpi | ~174 KB |
| `vector_only.pdf` | a page of paths and type, so the image stage must find nothing at all | ~8 KB |
| `already_packed.pdf` | object streams, cross-reference stream, every content stream flated — no slack left, so `NoGain` | ~3 KB |

Together with the `encrypted/` and `signed/` corpora above — which compression
refuses and hands back byte for byte — these are the six cases B24's T-197
calls for.

Regenerate with:

```sh
cargo run -p gen-fixtures -- compress
```

Name the corpus. A bare `cargo run -p gen-fixtures` rewrites all three, and
the signed one mints fresh keys on every run — four unrelated binary diffs in
a change that had nothing to do with signing.

**Committed, unlike `large/`.** The perf fixture is `.gitignore`d because it
is ~50MB, and `core/pdf-compress/tests/common/mod.rs` treats generated rows as
optional — a checkout that has not run the generator simply skips them. CI
never runs `gen-fixtures`, so a compression fixture kept out of the repository
would be a guardian that never fires. Every raster here is therefore a smooth
synthetic one, which keeps the whole corpus around half a megabyte.

Unlike the signed corpus, this generator **is byte-reproducible** — nothing in
it is random — so `compress_corpus.rs` pins the committed files to it. A
failure there means a builder was edited without regenerating.

Consumed by `core/pdf-compress/tests/`: `corpus.rs` (the guardians),
`image_stage.rs` (what each row was added to prove), `measure.rs` (the size
table) and `render_unchanged.rs` (pixel-for-pixel). Adding a row to
`common::CORPUS` is enough — all four pick it up.
