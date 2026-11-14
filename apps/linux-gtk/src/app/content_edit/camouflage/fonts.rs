//! Which font family the editor draws in: the PDF's own embedded program,
//! loaded into Pango, or a system family matched from the `/BaseFont` name.
//!
//! An embedded program is written to a private (0700, ours, never a
//! symlink) per-process directory under `$XDG_RUNTIME_DIR` or the system
//! temp dir, as a 0600 file created exclusively, and handed to `pango_font_map_add_font_file` (Pango
//! 1.56). That function is looked up at run time rather than linked: the
//! shell still has to build and run against the Pango 1.52 the CI image
//! ships, where the symbol does not exist — there the fallback family is
//! all there is.
//!
//! Registration is cached by the program's SHA-256 for the life of the
//! process: fontconfig keeps reading the file, so it must stay on disk, and
//! a second editor over the same font finds the family without writing or
//! registering anything. The directory goes when the application shuts down
//! ([`remove_font_files`]); one a crashed process left behind is swept the
//! next time a font is written.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CString};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use gtk::glib::translate::{from_glib_full, ToGlibPtr};
use gtk::{glib, pango, prelude::*};
use pdf_edit::{FontProgram, FontProgramFormat};
use sha2::{Digest, Sha256};

/// The prefix of the per-process directory the font files go in; the
/// process id follows it.
const DIRECTORY_PREFIX: &str = "vitela-fonts-";

thread_local! {
    /// Program digest to the family it registered as — `None` when it was
    /// tried and Pango would not take it, so a bad program is not rewritten
    /// and retried on every open.
    static REGISTERED: RefCell<HashMap<[u8; 32], Option<String>>> = RefCell::new(HashMap::new());
}

/// The family the editor asks Pango for, and the weight and slant the run
/// is drawn with.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FontChoice {
    /// A Pango/CSS family list, most wanted first: the registered program's
    /// family alone, or a name from `/BaseFont` followed by a generic.
    pub(crate) families: Vec<String>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
}

/// What the core read of a run's look beyond its font's name
/// (`pdf_edit::TextRunStyle`): the weight and slant its font descriptor
/// declares, and its text render mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FaceHints {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// `Tr`: `2` fills and strokes, which is how a bold face is faked.
    pub(crate) render_mode: u8,
}

/// `Tr` modes that stroke the glyphs' outlines (PDF 32000-1 §9.3.6). A
/// stroke over a fill thickens the text: the usual faked bold.
const STROKING_RENDER_MODES: [u8; 4] = [1, 2, 5, 6];

/// The font to draw a run whose font is `base_font` in: `registered`, the
/// family its embedded program loaded as, when there is one; otherwise the
/// system family the name suggests.
///
/// The weight and slant are asked for either way. A registered family does
/// not pin a face: Word embeds a regular and a bold subset that both call
/// themselves "Arial", and only the weight tells fontconfig which of them
/// to draw. Nor does asking for bold embolden a bold program twice —
/// fontconfig synthesizes bold only when the face it matched is lighter
/// than asked. GTK draws no stroke, so a stroking render mode is asked for
/// as bold too: the closest it can get.
pub(crate) fn choose(base_font: &str, registered: Option<String>, hints: FaceHints) -> FontChoice {
    let name = without_subset_tag(base_font);
    let lower = name.to_ascii_lowercase();
    let bold = hints.bold
        || STROKING_RENDER_MODES.contains(&hints.render_mode)
        || ["bold", "black", "heavy", "semibold", "demi"]
            .iter()
            .any(|word| lower.contains(word));
    let italic = hints.italic || lower.contains("italic") || lower.contains("oblique");
    let families = match registered {
        Some(family) => vec![family],
        None => fallback_families(name),
    };
    FontChoice {
        families,
        bold,
        italic,
    }
}

/// `ABCDEF+Name` without its six-letter subset tag.
fn without_subset_tag(base_font: &str) -> &str {
    match base_font.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase()) => rest,
        _ => base_font,
    }
}

/// A family list for a font with no usable program: the name's family part
/// (`Arial` of `Arial,BoldItalic`, `Times` of `Times-Roman`), which the
/// system may have or alias, then the generic family it belongs to.
fn fallback_families(name: &str) -> Vec<String> {
    let family = name
        .split([',', '-'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let lower = name.to_ascii_lowercase();
    let generic = if ["courier", "mono", "consol", "typewriter"]
        .iter()
        .any(|word| lower.contains(word))
    {
        "monospace"
    } else if ["times", "serif", "roman", "georgia", "garamond", "book"]
        .iter()
        .any(|word| lower.contains(word))
        && !lower.contains("sans")
    {
        "serif"
    } else {
        "sans-serif"
    };
    let mut families = Vec::new();
    if !family.is_empty() {
        families.push(family);
    }
    families.push(generic.to_string());
    families
}

/// The family `program` registers as in `font_map`, registering it on the
/// first ask. `None` when it cannot be: an old Pango, a program Pango
/// rejects, or one whose family name was already taken by a system font.
pub(crate) fn register(font_map: &pango::FontMap, program: &FontProgram) -> Option<String> {
    let digest: [u8; 32] = Sha256::digest(&program.bytes).into();
    if let Some(known) = REGISTERED.with(|registered| registered.borrow().get(&digest).cloned()) {
        return known;
    }
    let family = write_and_register(font_map, program, &digest);
    REGISTERED.with(|registered| {
        registered.borrow_mut().insert(digest, family.clone());
    });
    family
}

fn write_and_register(
    font_map: &pango::FontMap,
    program: &FontProgram,
    digest: &[u8; 32],
) -> Option<String> {
    let add = add_font_file_symbol()?;
    let directory = font_directory()?;
    let name: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let path = directory.join(format!("{name}.{}", extension(program.format)));
    write_font(&path, &program.bytes)?;

    let before = family_names(font_map);
    if !add(font_map, &path) {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    let mut added = family_names(font_map)
        .into_iter()
        .filter(|family| !before.contains(family));
    match (added.next(), added.next()) {
        (Some(family), None) => Some(family),
        // Nothing new: the family name is one the system already has, and
        // asking for it could draw either. Several: not one font.
        _ => None,
    }
}

fn family_names(font_map: &pango::FontMap) -> Vec<String> {
    font_map
        .list_families()
        .iter()
        .map(|family| family.name().to_string())
        .collect()
}

fn extension(format: FontProgramFormat) -> &'static str {
    match format {
        FontProgramFormat::TrueType => "ttf",
        FontProgramFormat::OpenType => "otf",
        FontProgramFormat::Cff => "cff",
        FontProgramFormat::Type1 => "pfa",
    }
}

/// Whether this Pango can load a font file at all (1.56 or later).
#[cfg(test)]
pub(crate) fn can_register() -> bool {
    add_font_file_symbol().is_some()
}

/// `pango_font_map_add_font_file`, as a callable, when the running Pango
/// has it.
fn add_font_file_symbol() -> Option<impl Fn(&pango::FontMap, &Path) -> bool> {
    type AddFontFile = unsafe extern "C" fn(
        *mut pango::ffi::PangoFontMap,
        *const c_char,
        *mut *mut glib::ffi::GError,
    ) -> glib::ffi::gboolean;
    extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    // SAFETY: `RTLD_DEFAULT` is the null handle on glibc, and the name is a
    // NUL-terminated literal. A non-null result is the Pango function of
    // that name, whose C signature `AddFontFile` spells out.
    let symbol = unsafe {
        dlsym(
            std::ptr::null_mut(),
            c"pango_font_map_add_font_file".as_ptr(),
        )
    };
    if symbol.is_null() {
        return None;
    }
    // SAFETY: see above — the symbol is that function.
    let add: AddFontFile = unsafe { std::mem::transmute::<*mut c_void, AddFontFile>(symbol) };
    Some(move |font_map: &pango::FontMap, path: &Path| {
        let Some(path) = path.to_str().and_then(|path| CString::new(path).ok()) else {
            return false;
        };
        let mut error: *mut glib::ffi::GError = std::ptr::null_mut();
        // SAFETY: a live font map, a NUL-terminated path, and an error
        // out-pointer that Pango fills on failure and we then own.
        let ok = unsafe { add(font_map.to_glib_none().0, path.as_ptr(), &mut error) };
        if !error.is_null() {
            // SAFETY: Pango handed the error over; this frees it.
            let _: glib::Error = unsafe { from_glib_full(error) };
        }
        ok != glib::ffi::GFALSE
    })
}

/// This process's font directory, created on first use — sweeping any a
/// process that no longer runs left behind.
fn font_directory() -> Option<PathBuf> {
    font_directory_in(&base_directory())
}

/// `$XDG_RUNTIME_DIR` — per user, 0700 — when the session has one.
fn base_directory() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|directory| directory.is_absolute() && directory.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// [`font_directory`] under `base`. A directory already there by that name
/// is used only when it is ours: in a shared temp dir anyone can plant one,
/// or a symlink to one, to swap the fonts or redirect the writes.
fn font_directory_in(base: &Path) -> Option<PathBuf> {
    let directory = own_directory(base);
    match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => sweep_stale_directories(base),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return None,
    }
    is_ours(&directory, true).then_some(directory)
}

/// Writes a 0600 file that did not exist — `create_new` does not follow a
/// symlink planted at `path`. One already there is a program this process
/// wrote earlier (names are digests) when it is a regular file of ours.
fn write_font(path: &Path, bytes: &[u8]) -> Option<()> {
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path);
    match created {
        Ok(mut file) => file.write_all(bytes).ok(),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            is_ours(path, false).then_some(())
        }
        Err(_) => None,
    }
}

/// Whether `path` itself — a symlink is never followed — is a directory
/// (or a regular file) this user owns that no one else can open.
fn is_ours(path: &Path, directory: bool) -> bool {
    extern "C" {
        fn geteuid() -> u32;
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let kind = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    // SAFETY: `geteuid` takes nothing and cannot fail.
    kind && metadata.uid() == unsafe { geteuid() } && metadata.mode() & 0o077 == 0
}

fn own_directory(base: &Path) -> PathBuf {
    base.join(format!("{DIRECTORY_PREFIX}{}", std::process::id()))
}

fn sweep_stale_directories(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix(DIRECTORY_PREFIX))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if !Path::new(&format!("/proc/{pid}")).exists() && is_ours(&entry.path(), true) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Removes this process's font files. For application shutdown: the fonts
/// they registered are unusable afterwards.
pub(crate) fn remove_font_files() {
    let directory = own_directory(&base_directory());
    if is_ours(&directory, true) {
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: FaceHints = FaceHints {
        bold: false,
        italic: false,
        render_mode: 0,
    };

    /// Word embeds a regular and a bold subset that both call themselves
    /// "Arial": the family alone cannot tell them apart, so the weight and
    /// slant still have to be asked for. Fontconfig only synthesizes bold
    /// when the face it matched is lighter than asked, so a bold program
    /// asked for at 700 is not emboldened twice.
    #[test]
    fn a_registered_program_keeps_the_weight_and_slant_it_was_named_with() {
        let choice = choose(
            "ABCDEF+Arial-BoldItalicMT",
            Some("Arial".to_string()),
            PLAIN,
        );
        assert_eq!(choice.families, ["Arial"]);
        assert!(choice.bold && choice.italic);

        let regular = choose("ABCDEF+ArialMT", Some("Arial".to_string()), PLAIN);
        assert!(!regular.bold && !regular.italic);
    }

    /// A subset named `ABCDEF+F1` says nothing; its descriptor still does.
    #[test]
    fn the_cores_reading_of_the_descriptor_makes_it_bold_or_italic() {
        let bold = choose(
            "ABCDEF+F1",
            Some("F1".to_string()),
            FaceHints {
                bold: true,
                ..PLAIN
            },
        );
        assert!(bold.bold && !bold.italic);

        let italic = choose(
            "ABCDEF+F1",
            None,
            FaceHints {
                italic: true,
                ..PLAIN
            },
        );
        assert!(italic.italic && !italic.bold);
    }

    /// Text that is stroked as well as filled (`2 Tr`) is a faked bold;
    /// GTK draws no stroke, so a bold weight is the closest it gets.
    #[test]
    fn a_stroking_render_mode_reads_as_bold() {
        for mode in [1, 2, 5, 6] {
            let choice = choose(
                "ArialMT",
                None,
                FaceHints {
                    render_mode: mode,
                    ..PLAIN
                },
            );
            assert!(choice.bold, "Tr {mode}");
        }
        for mode in [0, 3, 4, 7] {
            let choice = choose(
                "ArialMT",
                None,
                FaceHints {
                    render_mode: mode,
                    ..PLAIN
                },
            );
            assert!(!choice.bold, "Tr {mode}");
        }
    }

    #[test]
    fn standard_names_fall_back_to_their_generic_family() {
        let helvetica = choose("Helvetica", None, PLAIN);
        assert_eq!(helvetica.families, ["Helvetica", "sans-serif"]);
        assert!(!helvetica.bold && !helvetica.italic);

        let times = choose("Times-BoldItalic", None, PLAIN);
        assert_eq!(times.families, ["Times", "serif"]);
        assert!(times.bold && times.italic);

        let courier = choose("Courier-Oblique", None, PLAIN);
        assert_eq!(courier.families, ["Courier", "monospace"]);
        assert!(courier.italic);

        let arial = choose("Arial,Bold", None, PLAIN);
        assert_eq!(arial.families, ["Arial", "sans-serif"]);
        assert!(arial.bold);
    }

    #[test]
    fn the_subset_tag_is_not_part_of_the_family() {
        let choice = choose("QWERTY+DejaVuSans-Oblique", None, PLAIN);
        assert_eq!(choice.families, ["DejaVuSans", "sans-serif"]);
        assert!(choice.italic);
        // Not six capitals: not a tag.
        assert_eq!(
            choose("Ab+Thing", None, PLAIN).families,
            ["Ab+Thing", "sans-serif"]
        );
    }

    /// A fresh directory under the test binary's target dir — never the
    /// real temp dir, which the tests must not litter or race on — removed
    /// again when the guard drops.
    ///
    /// Removal matters beyond tidiness: CI runs the suite as root inside a
    /// network namespace, and a 0700 directory left under `target/` is one
    /// the runner user cannot read, so the cache step's
    /// `hashFiles('**/Cargo.lock')` walk fails the job afterwards.
    struct Scratch(PathBuf);

    impl std::ops::Deref for Scratch {
        type Target = PathBuf;
        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let exe = std::env::current_exe().expect("the test binary");
        let base = exe
            .parent()
            .expect("a deps dir")
            .join(format!("font-dir-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("the scratch dir");
        Scratch(base)
    }

    #[test]
    fn a_scratch_directory_is_removed_when_its_guard_drops() {
        let path = {
            let base = scratch("guard");
            font_directory_in(&base).expect("a fresh directory");
            base.0.clone()
        };
        assert!(!path.exists(), "no private directory is left under target/");
    }

    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::symlink_metadata(path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn the_font_directory_and_its_files_are_private() {
        let base = scratch("private");
        let directory = font_directory_in(&base).expect("a fresh directory");
        assert_eq!(mode(&directory), 0o700);
        let font = directory.join("font.ttf");
        write_font(&font, b"font").expect("a fresh file");
        assert_eq!(mode(&font), 0o600);
        assert_eq!(std::fs::read(&font).unwrap(), b"font");
        // Written again (content-addressed: the same bytes), it is reused.
        assert!(write_font(&font, b"font").is_some());
        assert!(font_directory_in(&base).is_some(), "our own, found again");
    }

    #[test]
    fn a_planted_directory_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let base = scratch("planted");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, own_directory(&base)).unwrap();
        assert_eq!(font_directory_in(&base), None, "a symlink is not ours");

        let base = scratch("open");
        std::fs::create_dir(own_directory(&base)).unwrap();
        std::fs::set_permissions(own_directory(&base), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        assert_eq!(font_directory_in(&base), None, "others can write into it");
    }

    #[test]
    fn a_planted_font_symlink_is_not_followed() {
        let base = scratch("symlink");
        let directory = font_directory_in(&base).unwrap();
        let victim = base.join("victim");
        std::fs::write(&victim, b"victim").unwrap();
        let font = directory.join("font.ttf");
        std::os::unix::fs::symlink(&victim, &font).unwrap();
        assert_eq!(write_font(&font, b"font"), None);
        assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
    }
}
