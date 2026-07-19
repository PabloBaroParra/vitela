//! Linux GTK4 shell.
//!
//! This dogfood client links the Rust rendering core directly rather than
//! crossing the `pdf-ffi` UniFFI boundary used by other platform shells.
//!
//! The shell is split by responsibility under [`app`]: window bootstrap and
//! wiring live in `app`, shared types in `app::state`, and each feature (open,
//! layout, render, print, search) in its own module. See the workspace root
//! `CLAUDE.md` for the "no monolithic shell" convention this structure follows.

#[cfg(target_os = "linux")]
mod app;

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    app::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("linux-gtk is available only on Linux.");
}
