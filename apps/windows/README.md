# Windows shell (stub)

Placeholder for the C#/WinUI3 application. Not a Rust crate — not part of the
Cargo workspace.

Implementation lands in Batch 10 (T-060..T-064), gated on the Batch 1
uniffi-bindgen-cs spike's go/no-go decision (T-010): WinUI3 app consuming
the `pdf-ffi` C# bindings (uniffi-bindgen-cs, or the `cbindgen` + P/Invoke
fallback) for open/render/scroll/zoom, password prompt, text select/search,
annotation toolbar, undo/redo, `PrintDocument` printing, WinRT
`Clipboard`/`DataPackage` paste + drag-and-drop, and `.dll` bundling with
Authenticode signing wired into `windows.yml`.
