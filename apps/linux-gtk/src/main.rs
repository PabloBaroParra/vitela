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
mod package_smoke;

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    let mut args = std::env::args_os();
    let _program = args.next();
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--package-smoke")) {
        let Some(receipt) = args.next() else {
            eprintln!("--package-smoke requires a receipt path");
            return gtk::glib::ExitCode::FAILURE;
        };
        if args.next().is_some() {
            eprintln!("--package-smoke accepts exactly one receipt path");
            return gtk::glib::ExitCode::FAILURE;
        }
        return match package_smoke::write_receipt(std::path::Path::new(&receipt)) {
            Ok(()) => gtk::glib::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("package smoke failed: {error}");
                gtk::glib::ExitCode::FAILURE
            }
        };
    }
    app::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("linux-gtk is available only on Linux.");
}
