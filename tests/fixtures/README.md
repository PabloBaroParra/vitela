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
