# Preserve cleartext signature Contents in encrypted PDFs

This is the unmodified crates.io `lopdf` 0.45.0 release archive, except for the
signature exception and owner-password key derivation in `src/encryption.rs`,
the recovery helper extraction in `src/encryption/algorithms.rs`, and this note.

Archive: `https://static.crates.io/crates/lopdf/lopdf-0.45.0.crate`.
Original Cargo.lock SHA-256:
`bfffda0fe1ab0157e1a13c14bebd3f28671f2fccb7922f0722ec53926e6922d3`.

ISO 32000-1 §7.6.1 excludes a signature dictionary's `/Contents` string from
encryption. The upstream recursive encrypt/decrypt functions processed that
string normally, corrupting the signature placeholder and then attempting to
decrypt cleartext CMS on reopen. Both directions now skip only `/Contents`
when `/Type` is `/Sig`; other dictionary strings remain encrypted.

For security-handler revisions 2–4, opening as owner must derive the file key
from the padded user password recovered from `/O` (Algorithm 7), not from the
owner password directly. The upstream authentication function already recovers
and verifies that value; the local helper returns it so `EncryptionState::decode`
can use the correct key. Without this fix, a signed owner-open revision reopens
under the user password with corrupted strings even after fixing `/Contents`.

The workspace patches crates.io to this local copy so save, signing and opening
all use the same rule. Remove the patch when an upstream release carries this
exception, after running the encrypted production-pipeline regression in
`core/pdf-sign/src/orchestrate.rs` and the FFI sign/reopen regression in
`core/pdf-ffi/tests/sign.rs`.
