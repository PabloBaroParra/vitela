# Contributing to Vitela

Thanks for taking the time. Vitela is a PDF editor that promises three things
its users cannot verify for themselves: that their documents never leave their
machine, that their files come back intact, and that existing signatures and
encryption survive an edit. Most of the rules below exist to keep those
promises true.

## Before you write code

**Open an issue first** for anything beyond an obvious fix. A short discussion
about approach costs minutes; a rejected PR costs you an afternoon. Bug reports
and feature requests both have templates.

Small, self-contained PRs get reviewed quickly. A PR that mixes a refactor, a
bug fix and a new feature will be sent back to be split, because there is no
honest way to review it as one unit.

## AI-assisted contributions

Most of this codebase was written with AI agents, so there is nothing to
apologise for in using them — the project's own agent rules live in
[CLAUDE.md](CLAUDE.md) and [AGENTS.md](AGENTS.md). What does not change is who
is accountable: **you are, for every line in your PR.**

That means you read the whole diff before opening it, you can explain why each
change is there, and you ran the gates below yourself — a green CI run on
generated code you have not read is not review, it is luck. Say in the PR
description if a change is largely AI-generated. It is not held against you; it
tells the reviewer where to look hardest.

## Ground rules

These are not style preferences. A PR that breaks one of them cannot be merged,
however good the rest of it is.

1. **No network calls. Ever.** Vitela is offline-first with zero telemetry, and
   CI enforces it by running the suite inside a network namespace with no
   routes (the `zero-network enforcement` job). A dependency that phones home is
   as much of a bug as one we write ourselves.
2. **Documents come back intact.** Annotations authored by other editors are
   preserved, encryption is re-applied on save rather than silently stripped,
   and saves are incremental where possible so existing digital signatures are
   not invalidated.
3. **Standards-compliant output.** Anything Vitela writes must render correctly
   in Acrobat, Preview and other spec-compliant viewers — not just in Vitela.
4. **All document logic lives in Rust.** Platform shells are thin UI over the
   same core. If you find yourself reimplementing document behaviour in Swift,
   Kotlin or C#, that logic belongs in a `core/` crate instead.

## Getting set up

Rust **stable** (edition 2021). There is no pinned toolchain file; CI tracks
stable, so please build against it.

The renderer needs a PDFium binary, which is never committed. Point
`PDFIUM_DYNAMIC_LIB_PATH` at your copy, or drop it in
`core/pdf-render/vendor/pdfium/` — see that directory's README. Prebuilt
binaries come from [`bblanchon/pdfium-binaries`](https://github.com/bblanchon/pdfium-binaries).

On Linux, the GTK4 shell needs development headers:

```bash
sudo apt-get install -y libgtk-4-dev
```

## The gates

Run these before you open a PR. They are exactly what CI runs, so a green local
run means a green `core` job:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
```

If you touched the README status tables:

```bash
python3 scripts/check_readme_tables.py
```

For advisory size, control-flow, and duplication review prompts:

```bash
python3 scripts/check_maintainability.py
```

See [Maintainability review prompts](docs/maintainability.md) for how to
justify a retained warning or change a narrowly scoped exclusion. These prompts
inform review; they are not a substitute for preserving behaviour with tests.

Shell-specific gates, when you touched that shell:

```bash
bash apps/macos/Tests/test-build-macos.sh
bash apps/macos/Tests/test-bundle-integrity.sh
bash apps/ios/Tests/test-build-ios.sh
```

### You can only verify your own platform

Shell crates are gated with `#[cfg(target_os = "...")]` and their GUI
dependencies are target-scoped, so `apps/linux-gtk` simply does not compile on
Windows — `cargo build` there skips it entirely. On the wrong host the most you
can check is `cargo fmt`, which still parses every module. The real compile and
test gate for a shell runs in that platform's CI workflow.

This is expected, not a broken setup. Say so in the PR: *"formatting only, macOS
gate unverified locally"* is a useful sentence. Claiming a gate passed when you
could not run it is not.

## Architecture rules

### Shells are never one file

Every app under `apps/` is split by responsibility from the start, not "once it
gets big". The reference layout is `apps/linux-gtk/src/`: `main.rs` declares
modules and launches, nothing else; `app/mod.rs` bootstraps and wires signals;
`app/state.rs` holds types that cross module boundaries; then one module per
feature. A file growing past a few hundred lines is the signal to split it, not
to keep appending. Full rationale in [CLAUDE.md](CLAUDE.md).

### The Apple shells share a core

`apps/apple/Shared/` is compiled into **both** the macOS and iOS targets, so it
may import only Foundation and CoreGraphics. The moment it imports AppKit or
UIKit, the other platform stops compiling. Platform chrome — app entry, file
picking, views — stays in `apps/macos/` and `apps/ios/`.

### Keep the status tables honest

`README.md` tracks per-capability status for each shell. When a capability ships
in a shell **and its tests pass**, flip its cell. `✅` means done and tested;
`🚧` means in progress; `—` means not yet. Marking something `✅` because the
code exists, without a test that proves it, is the one kind of documentation
change that will get a PR rejected outright.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/), with a scope where
one applies:

```
feat(ios): SwiftUI shell over the shared Apple viewer core
fix(ci): run Android tests through the Gradle wrapper
ci(deps): ignore RustCrypto major bumps that cms 0.2 cannot follow
docs: mark completed Linux/Windows shell tasks in B8/B10
```

Scopes in use: `core`, `ffi`, `linux`, `windows`, `macos`, `ios`, `android`,
`apple`, `ci`, `deps`, `docs`.

Explain **why** in the body, not what — the diff already says what. If a fix is
subtle, the commit message is where the next person finds out what went wrong
the first time.

## Pull requests

1. Fork, then branch from `main`. Branch names like `feat/ios-text-search` or
   `fix/render-race`.
2. Push to your fork and open a PR against `main`. Fill in the template.
3. CI runs automatically. Fork PRs do not receive repository secrets — that is
   deliberate.
4. A maintainer reviews. `main` is protected: no direct pushes, every change
   arrives through a reviewed PR.
5. PRs are squash-merged, so your branch becomes one commit. Write the PR title
   as the commit message you want in the history.

Expect review comments. They are about the code, not about you, and "why did
you do it this way?" is a genuine question — a good answer often ends the
thread.

## Reporting a vulnerability

Do **not** open a public issue. See [SECURITY.md](SECURITY.md).

## Licence

Vitela is dual-licensed under [Apache 2.0](LICENSE-APACHE) and
[MIT](LICENSE-MIT). By contributing, you agree that your contribution is
licensed under both, with no additional terms.
