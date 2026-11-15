//! The sidecar beside each recovery entry's bytes: when they were written,
//! what the document was called, and where it came from.
//!
//! A small line-oriented text format, `field TAB value`, under a versioned
//! header. Fields this version does not know are skipped, so a later version
//! can add one without making older builds throw its entries away.

use std::path::PathBuf;

use gtk::prelude::FileExt;
use gtk::{gio, glib};

/// First line of every sidecar. A different first line is a different format,
/// and is discarded rather than guessed at.
const META_HEADER: &str = "vitela-recovery 1";

/// What the sidecar records about the bytes beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Meta {
    /// When the bytes were written, in Unix seconds.
    pub(crate) saved_at: i64,
    /// The document's display name (`DocumentSession::base_name`).
    pub(crate) name: String,
    /// The file the document was opened from or last saved to, when it has
    /// one. A new or sample document has none.
    pub(crate) origin: Option<PathBuf>,
    /// Whether the bytes already broke a signature the document's file
    /// carries — a snapshot taken after an edit the rewriter had to apply.
    /// A restored document carries this into its session, so its Save still
    /// asks before writing over the signed original.
    pub(crate) breaks_signatures: bool,
}

/// The sidecar field for [`Meta::breaks_signatures`], written only when it
/// is true: a sidecar from before the field existed reads as false.
const BREAKS_SIGNATURES: &str = "breaks-signatures";

impl Meta {
    /// One `field TAB value` line each. The name is URI-escaped and the origin
    /// stored as a `file://` URI, because both are the user's text and a
    /// Linux file name may hold a tab or a newline.
    pub(super) fn encode(&self) -> String {
        let mut text = format!(
            "{META_HEADER}\nsaved\t{}\nname\t{}\n",
            self.saved_at,
            glib::Uri::escape_string(&self.name, None, true)
        );
        if let Some(origin) = &self.origin {
            text.push_str(&format!("origin\t{}\n", gio::File::for_path(origin).uri()));
        }
        if self.breaks_signatures {
            text.push_str(&format!("{BREAKS_SIGNATURES}\t1\n"));
        }
        text
    }

    /// `None` only for a different format or a sidecar missing the time or
    /// the name — the fields an entry cannot be offered without. Any other
    /// line that does not read — a field from a later version, a value this
    /// one cannot parse, a line with no tab at all — is skipped: refusing
    /// the sidecar would delete the user's only copy of their work over it.
    pub(super) fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != META_HEADER {
            return None;
        }
        let (mut saved_at, mut name, mut origin) = (None, None, None);
        let mut breaks_signatures = false;
        for line in lines {
            let Some((field, value)) = line.split_once('\t') else {
                continue;
            };
            match field {
                "saved" => saved_at = value.parse().ok(),
                "name" => {
                    name = glib::Uri::unescape_string(value, None::<&str>)
                        .map(|name| name.to_string());
                }
                "origin" => origin = gio::File::for_uri(value).path(),
                BREAKS_SIGNATURES => breaks_signatures = value == "1",
                _ => {}
            }
        }
        Some(Self {
            saved_at: saved_at?,
            name: name?,
            origin,
            breaks_signatures,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_meta() -> Meta {
        Meta {
            saved_at: 1_700_000_000,
            name: "quote.pdf".to_owned(),
            origin: Some(PathBuf::from("/home/someone/quote.pdf")),
            breaks_signatures: false,
        }
    }

    #[test]
    fn a_sidecar_round_trips() {
        let meta = a_meta();

        assert_eq!(Meta::parse(&meta.encode()), Some(meta));
    }

    #[test]
    fn the_signature_flag_round_trips() {
        let meta = Meta {
            breaks_signatures: true,
            ..a_meta()
        };

        assert_eq!(Meta::parse(&meta.encode()), Some(meta));
    }

    /// Sidecars written before the flag existed have no line for it, and
    /// still read: their bytes broke nothing this version knows about.
    #[test]
    fn a_sidecar_without_the_signature_flag_reads_as_breaking_nothing() {
        let text = format!(
            "{META_HEADER}
saved	5
name	quote.pdf
"
        );

        let meta = Meta::parse(&text).expect("an older sidecar still reads");

        assert!(!meta.breaks_signatures);
    }

    /// A line this version cannot make sense of costs that line, never the
    /// entry: deleting the user's only copy over a stray line is the worse
    /// failure.
    #[test]
    fn unknown_and_malformed_lines_are_skipped() {
        let text = format!(
            "{META_HEADER}
saved	5
no tab on this line
later-field	value
             origin	not a uri
name	quote.pdf
"
        );

        let meta = Meta::parse(&text).expect("the required fields are all there");

        assert_eq!(meta.saved_at, 5);
        assert_eq!(meta.name, "quote.pdf");
        assert_eq!(meta.origin, None, "an unreadable origin is only absent");
    }

    #[test]
    fn a_sidecar_missing_a_required_field_is_refused() {
        assert_eq!(
            Meta::parse(&format!(
                "{META_HEADER}
name	quote.pdf
"
            )),
            None
        );
        assert_eq!(
            Meta::parse(&format!(
                "{META_HEADER}
saved	5
"
            )),
            None
        );
        assert_eq!(
            Meta::parse(&format!(
                "{META_HEADER}
saved	soon
name	quote.pdf
"
            )),
            None,
            "a time that does not parse is a time that is missing"
        );
    }

    #[test]
    fn a_different_header_is_a_different_format() {
        assert_eq!(
            Meta::parse(
                "vitela-recovery 2
saved	5
name	q
"
            ),
            None
        );
    }
}
