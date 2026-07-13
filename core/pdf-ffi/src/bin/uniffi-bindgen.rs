//! CLI entrypoint for generating foreign-language bindings from the
//! compiled `pdf-ffi` library (T-042). Built only with the `bindgen`
//! feature so the CLI's dependency tree (clap et al.) never leaks into the
//! library consumed by the shells. Library-mode invocation (macos.yml):
//!
//! ```sh
//! cargo run -p pdf-ffi --features bindgen --bin uniffi-bindgen -- \
//!     generate --library target/debug/libpdf_ffi.dylib \
//!     --language swift --out-dir target/bindings/swift
//! ```
//!
//! C# bindings (Batch 10) use the external `uniffi-bindgen-cs` tool against
//! the same cdylib instead — see spikes/uniffi-cs/README.md.

fn main() {
    uniffi::uniffi_bindgen_main()
}
