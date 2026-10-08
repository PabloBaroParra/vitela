//! Home's footer: which build of Vitela is running.
//!
//! The release tag is the only source of a real version
//! (`scripts/release-version.sh`), and `Cargo.toml`'s `version` never follows
//! it — it would read `0.1.0` on every beta. So the version is whatever the
//! build was handed in `VITELA_VERSION` (linux.yml sets it to the tag's semver
//! on a release), and any other build says it is a development one instead of
//! claiming a number it does not have.

use gtk::prelude::*;
use gtk::{Align, Label};

/// The version this binary was built as, if the build was given one.
const BUILD_VERSION: Option<&str> = option_env!("VITELA_VERSION");

/// The footer's text for a build `version`; an unset or blank one is `dev`.
fn version_label(version: Option<&str>) -> String {
    let version = version.map(str::trim).filter(|version| !version.is_empty());
    format!("Vitela {}", version.unwrap_or("dev"))
}

pub(super) fn build_version_footer() -> Label {
    let label = Label::new(Some(&version_label(BUILD_VERSION)));
    label.add_css_class("home-version");
    label.set_halign(Align::End);
    label.set_selectable(true);
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_build_shows_its_tag_version() {
        assert_eq!(version_label(Some("0.1.0-beta.3")), "Vitela 0.1.0-beta.3");
    }

    #[test]
    fn a_build_without_a_version_says_it_is_a_development_one() {
        assert_eq!(version_label(None), "Vitela dev");
        assert_eq!(version_label(Some("  ")), "Vitela dev");
    }
}
