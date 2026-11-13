//! Command palette (Ctrl+K), batch B27 — see `docs/batch-command-palette.md`.
//!
//! The pure fuzzy matcher (T-228) and the entry catalog (T-229) exist so
//! far. The overlay view and the `win.command-palette` wiring land in T-230.

mod entries;
mod matcher;
