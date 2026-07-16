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
