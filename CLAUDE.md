# Project conventions — pdf-editor-mvp

Guidance for any agent working in this repository. Workflow rules (commits,
handoff) live in [AGENTS.md](AGENTS.md); this file covers architecture.

## No monolithic shells

**A platform shell must never live in a single file.** Every shell app under
`apps/` is split by responsibility from the start — not "once it gets big".

The reference layout is `apps/linux-gtk/src/`:

- `main.rs` — entry point only (the `main` functions + `mod app;`). Nothing else.
- `app/mod.rs` — application bootstrap, signal wiring, shared constants.
- `app/state.rs` — shared data types (widget handles, session state, value types).
- one module per feature — `document`, `layout`, `render`, `print`, `search`.

Windows follows the same principle with its `Facade/` split
(`PdfDocumentFacade`, `Models`, `PdfCore`) plus the `MainWindow` view.

### Rules

1. `main.rs` / entry point stays minimal — declare modules and launch, no logic.
2. Group functions and types by responsibility, not by "leftover". If a file
   grows past a few hundred lines, that is the signal to split it, not to keep
   appending.
3. Types that cross module boundaries go in a shared `state`/`models` module;
   feature logic imports them.
4. Move a function's tests into the same module as the function.
5. Refactors that only relocate code must change zero behavior — verify with
   the platform's build + test gate before handing off.

### Why

The GTK4 shell was first written as a single 1187-line `main.rs`. It worked,
but every change meant scrolling one giant file and the module boundaries lived
only in the author's head. Splitting by responsibility makes ownership,
review, and testing obvious — and the same split is expected of every shell
that lands after it.

## Verification note (cross-platform)

Shell crates are gated with `#[cfg(target_os = "...")]` and their GUI
dependencies are target-scoped in `Cargo.toml`. A shell can only be
type-checked on its own platform (e.g. `linux-gtk` does not compile on
Windows — `cargo build` there skips it entirely). On the wrong host, the most
you can verify locally is formatting (`cargo fmt`, which parses every module);
the real compile + test gate runs in that platform's CI workflow.
