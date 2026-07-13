//! Resolves the pdfium dynamic library path (T-015 prerequisite).
//!
//! pdfium is distributed as a prebuilt platform binary (`bblanchon/pdfium-binaries`,
//! per `design.md` "PDFium Binary Distribution"), never vendored into the
//! Rust source tree. Resolution order, most to least specific:
//!
//! 1. `PDFIUM_DYNAMIC_LIB_PATH` env var — explicit override, used by shells
//!    that bundle the library at a known runtime path (deb/AppImage on
//!    Linux, `Frameworks/` on macOS, alongside the `.exe` on Windows).
//! 2. `<this crate>/vendor/pdfium/bin/<platform file name>` — a dev/test
//!    convenience location (gitignored, populated by downloading the
//!    matching `bblanchon/pdfium-binaries` release; see `vendor/pdfium/README.md`).
//! 3. The bare platform library name, resolved via the OS's normal dynamic
//!    linker search path (system-installed pdfium, if present).

use std::path::PathBuf;

use pdfium_render::prelude::Pdfium;

const ENV_OVERRIDE: &str = "PDFIUM_DYNAMIC_LIB_PATH";

pub fn resolve_library_path() -> PathBuf {
    if let Ok(path) = std::env::var(ENV_OVERRIDE) {
        return PathBuf::from(path);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendored = manifest_dir
        .join("vendor")
        .join("pdfium")
        .join("bin")
        .join(Pdfium::pdfium_platform_library_name());
    if vendored.exists() {
        return vendored;
    }

    PathBuf::from(Pdfium::pdfium_platform_library_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A single test function, not two: both cases mutate the process-wide
    // `PDFIUM_DYNAMIC_LIB_PATH` env var, and `cargo test` runs tests in
    // parallel threads within one process by default, so splitting this
    // into separate `#[test]` fns would be a data race on the env var.
    #[test]
    fn env_override_wins_and_fallback_resolves_a_bare_platform_name() {
        std::env::set_var(ENV_OVERRIDE, "/nonexistent/path/to/pdfium.so");
        let overridden = resolve_library_path();
        assert_eq!(overridden, PathBuf::from("/nonexistent/path/to/pdfium.so"));

        std::env::remove_var(ENV_OVERRIDE);
        // With no override, resolution either finds the vendored dev/test
        // copy or falls back to a bare platform library name — either way,
        // it must resolve to a non-empty file name.
        let resolved = resolve_library_path();
        assert!(resolved
            .file_name()
            .map(|f| !f.to_string_lossy().is_empty())
            .unwrap_or(false));
    }
}
