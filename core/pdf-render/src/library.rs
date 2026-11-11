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
    resolve_library_path_from(std::env::var(ENV_OVERRIDE).ok())
}

/// [`resolve_library_path`] with the env override passed in, so tests never
/// mutate the process-wide env var (other tests in the same process bind
/// pdfium through it concurrently).
fn resolve_library_path_from(env_override: Option<String>) -> PathBuf {
    if let Some(path) = env_override {
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

    #[test]
    fn env_override_wins() {
        let resolved = resolve_library_path_from(Some("/nonexistent/path/to/pdfium.so".into()));
        assert_eq!(resolved, PathBuf::from("/nonexistent/path/to/pdfium.so"));
    }

    #[test]
    fn fallback_resolves_to_a_non_empty_file_name() {
        // Either the vendored dev/test copy or the bare platform name.
        let resolved = resolve_library_path_from(None);
        assert!(resolved
            .file_name()
            .map(|f| !f.to_string_lossy().is_empty())
            .unwrap_or(false));
    }
}
