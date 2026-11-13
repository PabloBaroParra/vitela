//! Command palette (Ctrl+K), batch B27 — see `docs/batch-command-palette.md`.
//!
//! Only the pure fuzzy matcher exists so far (T-228). The entry catalog lands
//! in T-229 and the overlay view plus the `win.command-palette` wiring in T-230.

mod matcher;
