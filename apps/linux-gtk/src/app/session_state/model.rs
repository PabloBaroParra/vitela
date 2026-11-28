//! What is remembered, as values: the window's size, the side panels, and
//! where each document was being read. Pure — no widgets, no files, no
//! clock — so reading a damaged file and clamping a stale position are
//! tested as text in, values out.
//!
//! ## The file
//!
//! Plain text, one record a line, fields separated by tabs — the shape of
//! `home::hidden_recents`, for the same reasons: it can be read with `cat`,
//! and a line that does not parse costs that line and nothing else. Each
//! `⇥` below is one tab character.
//!
//! ```text
//! vitela-session-state ⇥ 1
//! window ⇥ 1280 ⇥ 900 ⇥ 0
//! panels ⇥ 1 ⇥ 0 ⇥ 360
//! document ⇥ 1795823029 ⇥ 4 ⇥ 0.2500 ⇥ fit-width ⇥ file:///home/user/report.pdf
//! ```
//!
//! * `window` — width, height, maximized.
//! * `panels` — pages column open, tools column open, tools column width.
//! * `document` — when it was last seen (Unix seconds), the page index, how
//!   far down that page, the zoom (`fit-width`, `fit-page` or a factor), and
//!   the file as a URI, which escapes the tabs and newlines a path may hold.
//!
//! Reading never fails. A line that is unknown, malformed or out of range is
//! skipped, so a file from a newer version, a truncated one and one that is
//! not this file at all each read as "whatever of it made sense", down to
//! [`State::default`].

use std::path::{Path, PathBuf};

use gtk::gio;
use gtk::prelude::FileExt;

use crate::app::layout::{ReadingPosition, Zoom};

/// The first line of the file, and the version of this format.
const HEADER: &str = "vitela-session-state\t1";

/// How many documents are remembered. Past this the ones seen longest ago
/// are dropped: the file is read at every start and must stay small.
pub(crate) const MAX_DOCUMENTS: usize = 64;

/// The window sizes worth restoring. Outside these the saved value is a
/// mistake — a file edited by hand, a display that is gone — and the
/// default size is the better answer.
const WINDOW_WIDTH: std::ops::RangeInclusive<i32> = 480..=16_384;
const WINDOW_HEIGHT: std::ops::RangeInclusive<i32> = 360..=16_384;
/// The tools column widths worth restoring.
const PANEL_WIDTH: std::ops::RangeInclusive<i32> = 1..=4_096;
/// The zoom factors the toolbar's ladder reaches (`step_zoom`).
const ZOOM_FACTOR: std::ops::RangeInclusive<f64> = 0.10..=8.0;

/// The window's size while it is not maximized, and whether it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowGeometry {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) maximized: bool,
}

/// The two side columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Panels {
    pub(crate) pages_open: bool,
    pub(crate) tools_open: bool,
    /// The tools column's width while open, or `None` when it was never
    /// measured — the default width then stands.
    pub(crate) tools_width: Option<i32>,
}

/// Where one document was being read.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct DocumentView {
    pub(crate) zoom: Zoom,
    pub(crate) reading: ReadingPosition,
    /// When it was recorded, in Unix seconds: what orders the list and
    /// settles which of two windows' records of one file is the newer.
    pub(crate) seen: i64,
}

/// Everything remembered between runs.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct State {
    pub(crate) window: Option<WindowGeometry>,
    pub(crate) panels: Option<Panels>,
    /// Most recently seen first, at most [`MAX_DOCUMENTS`].
    documents: Vec<(PathBuf, DocumentView)>,
}

fn flag(text: &str) -> Option<bool> {
    match text {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn zoom_text(zoom: Zoom) -> String {
    match zoom {
        Zoom::FitWidth => "fit-width".to_owned(),
        Zoom::FitPage => "fit-page".to_owned(),
        Zoom::Custom(factor) => format!("{factor:.4}"),
    }
}

fn parse_zoom(text: &str) -> Option<Zoom> {
    match text {
        "fit-width" => Some(Zoom::FitWidth),
        "fit-page" => Some(Zoom::FitPage),
        factor => factor
            .parse::<f64>()
            .ok()
            .filter(|factor| ZOOM_FACTOR.contains(factor))
            .map(Zoom::Custom),
    }
}

fn parse_window(fields: &[&str]) -> Option<WindowGeometry> {
    let [width, height, maximized] = fields else {
        return None;
    };
    Some(WindowGeometry {
        width: width
            .parse()
            .ok()
            .filter(|width| WINDOW_WIDTH.contains(width))?,
        height: height
            .parse()
            .ok()
            .filter(|height| WINDOW_HEIGHT.contains(height))?,
        maximized: flag(maximized)?,
    })
}

fn parse_panels(fields: &[&str]) -> Option<Panels> {
    let [pages_open, tools_open, tools_width] = fields else {
        return None;
    };
    Some(Panels {
        pages_open: flag(pages_open)?,
        tools_open: flag(tools_open)?,
        // An unusable width is not a reason to forget which columns were
        // open.
        tools_width: tools_width
            .parse()
            .ok()
            .filter(|width| PANEL_WIDTH.contains(width)),
    })
}

fn parse_document(fields: &[&str]) -> Option<(PathBuf, DocumentView)> {
    let [seen, page_index, fraction, zoom, uri] = fields else {
        return None;
    };
    let fraction: f64 = fraction
        .parse()
        .ok()
        .filter(|value: &f64| value.is_finite())?;
    let path = gio::File::for_uri(uri).path()?;
    Some((
        path,
        DocumentView {
            zoom: parse_zoom(zoom)?,
            reading: ReadingPosition {
                page_index: page_index.parse().ok()?,
                fraction: fraction.clamp(0.0, 1.0),
            },
            seen: seen.parse().ok()?,
        },
    ))
}

impl State {
    /// Reads a file's text. Never fails: see the module documentation.
    pub(crate) fn parse(text: &str) -> Self {
        let mut state = State::default();
        for line in text.lines() {
            let mut fields = line.split('\t');
            let kind = fields.next().unwrap_or_default();
            let fields: Vec<&str> = fields.collect();
            match kind {
                "window" => state.window = parse_window(&fields).or(state.window),
                "panels" => state.panels = parse_panels(&fields).or(state.panels),
                "document" => {
                    if let Some((path, view)) = parse_document(&fields) {
                        state.merge_document(path, view);
                    }
                }
                _ => {}
            }
        }
        state.settle();
        state
    }

    /// The file's text for this state.
    pub(crate) fn serialize(&self) -> String {
        let mut text = format!("{HEADER}\n");
        if let Some(window) = self.window {
            text.push_str(&format!(
                "window\t{}\t{}\t{}\n",
                window.width,
                window.height,
                u8::from(window.maximized)
            ));
        }
        if let Some(panels) = self.panels {
            text.push_str(&format!(
                "panels\t{}\t{}\t{}\n",
                u8::from(panels.pages_open),
                u8::from(panels.tools_open),
                panels.tools_width.unwrap_or(0)
            ));
        }
        for (path, view) in &self.documents {
            text.push_str(&format!(
                "document\t{}\t{}\t{:.4}\t{}\t{}\n",
                view.seen,
                view.reading.page_index,
                view.reading.fraction,
                zoom_text(view.zoom),
                gio::File::for_path(path).uri()
            ));
        }
        text
    }

    /// Where `path` was last being read, if it is remembered.
    pub(crate) fn document(&self, path: &Path) -> Option<DocumentView> {
        self.documents
            .iter()
            .find(|(known, _)| known == path)
            .map(|(_, view)| *view)
    }

    /// Records `view` for `path` as the most recent. `true` when that
    /// changed what is remembered — the timestamp alone does not count, so
    /// a document left where it was is not a reason to write the file.
    pub(crate) fn remember_document(&mut self, path: &Path, view: DocumentView) -> bool {
        let unchanged = self.documents.first().is_some_and(|(first, known)| {
            first == path && known.zoom == view.zoom && known.reading == view.reading
        });
        if unchanged {
            return false;
        }
        self.documents.retain(|(known, _)| known != path);
        self.documents.insert(0, (path.to_path_buf(), view));
        self.documents.truncate(MAX_DOCUMENTS);
        true
    }

    /// Takes in every document `other` remembers more recently than this
    /// state does — how two windows writing one file keep each other's
    /// documents instead of the last writer erasing the rest.
    pub(crate) fn merge_documents(&mut self, other: &State) {
        for (path, view) in &other.documents {
            self.merge_document(path.clone(), *view);
        }
        self.settle();
    }

    /// Keeps the newer of `view` and what is already known for `path`.
    fn merge_document(&mut self, path: PathBuf, view: DocumentView) {
        match self.documents.iter_mut().find(|(known, _)| *known == path) {
            Some((_, known)) if known.seen >= view.seen => {}
            Some((_, known)) => *known = view,
            None => self.documents.push((path, view)),
        }
    }

    /// Most recent first, and no more than [`MAX_DOCUMENTS`].
    fn settle(&mut self) {
        // Stable, so documents seen in the same second keep their order.
        self.documents
            .sort_by_key(|(_, view)| std::cmp::Reverse(view.seen));
        self.documents.truncate(MAX_DOCUMENTS);
    }

    #[cfg(test)]
    pub(crate) fn document_count(&self) -> usize {
        self.documents.len()
    }
}

/// `reading` as it applies to a document that now has `page_count` pages.
///
/// The file may have been replaced since the position was recorded. A page
/// that is no longer there resolves to the top of the last one that is —
/// the nearest place to where the reader was — rather than to whatever the
/// same fraction happens to be on a different page. `None` for a document
/// with no pages, which has nowhere to be.
pub(crate) fn clamp_reading(
    reading: ReadingPosition,
    page_count: usize,
) -> Option<ReadingPosition> {
    let last = page_count.checked_sub(1)?;
    if reading.page_index > last {
        return Some(ReadingPosition {
            page_index: last,
            fraction: 0.0,
        });
    }
    Some(ReadingPosition {
        page_index: reading.page_index,
        fraction: if reading.fraction.is_finite() {
            reading.fraction.clamp(0.0, 1.0)
        } else {
            0.0
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(page_index: usize, zoom: Zoom, seen: i64) -> DocumentView {
        DocumentView {
            zoom,
            reading: ReadingPosition {
                page_index,
                fraction: 0.25,
            },
            seen,
        }
    }

    fn sample() -> State {
        let mut state = State {
            window: Some(WindowGeometry {
                width: 1280,
                height: 900,
                maximized: true,
            }),
            panels: Some(Panels {
                pages_open: true,
                tools_open: false,
                tools_width: Some(412),
            }),
            ..State::default()
        };
        state.remember_document(Path::new("/home/user/a.pdf"), view(4, Zoom::FitPage, 100));
        state.remember_document(
            Path::new("/home/user/b.pdf"),
            view(0, Zoom::Custom(1.25), 200),
        );
        state
    }

    #[test]
    fn a_state_reads_back_as_it_was_written() {
        let state = sample();

        let read = State::parse(&state.serialize());

        assert!(read == state);
        assert_eq!(read.window, state.window);
        assert_eq!(read.panels, state.panels);
        let a = read.document(Path::new("/home/user/a.pdf")).expect("a.pdf");
        assert!(a.zoom == Zoom::FitPage);
        assert_eq!(a.reading.page_index, 4);
        assert_eq!(a.reading.fraction, 0.25);
        assert_eq!(a.seen, 100);
        let b = read.document(Path::new("/home/user/b.pdf")).expect("b.pdf");
        assert!(b.zoom == Zoom::Custom(1.25));
    }

    #[test]
    fn the_written_file_is_the_documented_text() {
        assert_eq!(
            sample().serialize(),
            "vitela-session-state\t1\n\
             window\t1280\t900\t1\n\
             panels\t1\t0\t412\n\
             document\t200\t0\t0.2500\t1.2500\tfile:///home/user/b.pdf\n\
             document\t100\t4\t0.2500\tfit-page\tfile:///home/user/a.pdf\n"
        );
    }

    /// A path may hold a tab or a newline; the URI escapes both, so the
    /// record stays one line of five fields.
    #[test]
    fn a_path_with_a_tab_a_newline_and_an_accent_survives() {
        let path = Path::new("/home/user/informe\tfinal\nversión 2.pdf");
        let mut state = State::default();
        state.remember_document(path, view(2, Zoom::FitWidth, 5));

        let text = state.serialize();
        assert_eq!(text.lines().count(), 2, "the header and one record");
        let read = State::parse(&text);

        assert_eq!(
            read.document(path)
                .expect("the document")
                .reading
                .page_index,
            2
        );
    }

    #[test]
    fn an_empty_or_missing_file_is_the_defaults() {
        assert!(State::parse("") == State::default());
        assert_eq!(State::default().window, None);
        assert_eq!(State::default().panels, None);
        assert_eq!(State::default().document_count(), 0);
    }

    #[test]
    fn a_file_that_is_not_this_file_is_the_defaults() {
        for text in [
            "%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n",
            "\u{0}\u{1}\u{2}\tbinary\tnoise\n\n\n",
            "{\"window\": {\"width\": 1280}}",
            "window\nwindow\t\npanels\t1\ndocument\t1\t2\n",
        ] {
            assert!(State::parse(text) == State::default(), "{text:?}");
        }
    }

    /// A bad line costs that line: what else the file holds is still read.
    #[test]
    fn a_damaged_line_is_skipped_and_the_rest_is_kept() {
        let text = "vitela-session-state\t1\n\
                    window\twide\t900\t0\n\
                    panels\t0\t1\t300\n\
                    document\tnot-a-time\t0\t0.5\tfit-width\tfile:///home/user/a.pdf\n\
                    document\t50\t3\t0.5\tfit-width\tfile:///home/user/b.pdf\n\
                    something-from-a-newer-version\t1\t2\t3\n";

        let read = State::parse(text);

        assert_eq!(read.window, None);
        assert_eq!(
            read.panels,
            Some(Panels {
                pages_open: false,
                tools_open: true,
                tools_width: Some(300),
            })
        );
        assert_eq!(read.document_count(), 1);
        assert!(read.document(Path::new("/home/user/b.pdf")).is_some());
    }

    /// A file cut short mid-line, as a crash during a write that was not
    /// atomic would leave it.
    #[test]
    fn a_truncated_file_keeps_its_whole_lines() {
        let whole = sample().serialize();
        let cut = &whole[..whole.len() - 30];

        let read = State::parse(cut);

        assert_eq!(read.window, sample().window);
        assert_eq!(read.panels, sample().panels);
        assert_eq!(read.document_count(), 1);
    }

    #[test]
    fn a_window_size_that_cannot_be_right_is_not_restored() {
        for line in [
            "window\t0\t0\t0",
            "window\t-1280\t900\t0",
            "window\t100\t900\t0",
            "window\t1280\t20\t0",
            "window\t999999\t900\t0",
            "window\t1280\t900\tyes",
            "window\t1280\t900",
            "window\t1280\t900\t0\textra",
            "window\t1280.5\t900\t0",
        ] {
            assert_eq!(State::parse(line).window, None, "{line}");
        }
    }

    /// The columns' state is worth keeping even when the width is not.
    #[test]
    fn a_panel_width_that_cannot_be_right_keeps_the_open_flags() {
        for width in ["0", "-40", "99999", "wide"] {
            let read = State::parse(&format!("panels\t0\t0\t{width}"));

            assert_eq!(
                read.panels,
                Some(Panels {
                    pages_open: false,
                    tools_open: false,
                    tools_width: None,
                }),
                "{width}"
            );
        }
    }

    #[test]
    fn a_zoom_outside_the_ladder_or_not_a_number_drops_the_record() {
        for zoom in ["0", "0.01", "64", "NaN", "inf", "-1", "huge", ""] {
            let read = State::parse(&format!(
                "document\t1\t0\t0.5\t{zoom}\tfile:///home/user/a.pdf"
            ));

            assert_eq!(read.document_count(), 0, "{zoom:?}");
        }
    }

    #[test]
    fn a_fraction_is_clamped_and_one_that_is_not_a_number_drops_the_record() {
        let read = State::parse("document\t1\t0\t7.5\tfit-width\tfile:///home/user/a.pdf");
        let kept = read.document(Path::new("/home/user/a.pdf")).expect("kept");
        assert_eq!(kept.reading.fraction, 1.0);

        for fraction in ["NaN", "inf", "half"] {
            let read = State::parse(&format!(
                "document\t1\t0\t{fraction}\tfit-width\tfile:///home/user/a.pdf"
            ));
            assert_eq!(read.document_count(), 0, "{fraction}");
        }
    }

    #[test]
    fn a_record_that_names_no_local_file_is_dropped() {
        for uri in ["https://example.org/a.pdf", "not a uri", ""] {
            let read = State::parse(&format!("document\t1\t0\t0.5\tfit-width\t{uri}"));

            assert_eq!(read.document_count(), 0, "{uri:?}");
        }
    }

    #[test]
    fn the_same_document_twice_keeps_the_newer_record() {
        let text = "document\t10\t1\t0.0\tfit-width\tfile:///home/user/a.pdf\n\
                    document\t30\t3\t0.0\tfit-width\tfile:///home/user/a.pdf\n\
                    document\t20\t2\t0.0\tfit-width\tfile:///home/user/a.pdf\n";

        let read = State::parse(text);

        assert_eq!(read.document_count(), 1);
        let kept = read.document(Path::new("/home/user/a.pdf")).expect("kept");
        assert_eq!((kept.seen, kept.reading.page_index), (30, 3));
    }

    #[test]
    fn remembering_a_document_puts_it_first_and_keeps_one_record_of_it() {
        let mut state = sample();
        let a = Path::new("/home/user/a.pdf");

        assert!(state.remember_document(a, view(9, Zoom::FitWidth, 300)));

        assert_eq!(state.document_count(), 2);
        assert_eq!(state.document(a).expect("a.pdf").reading.page_index, 9);
        assert!(state
            .serialize()
            .lines()
            .nth(3)
            .is_some_and(|line| line.ends_with("a.pdf")));
    }

    /// The timer fires on every scroll that settles; a view that did not
    /// move must not be a reason to write the file again.
    #[test]
    fn remembering_the_same_view_again_changes_nothing() {
        let mut state = sample();
        let b = Path::new("/home/user/b.pdf");

        assert!(!state.remember_document(b, view(0, Zoom::Custom(1.25), 999)));

        assert_eq!(state.document(b).expect("b.pdf").seen, 200);
    }

    #[test]
    fn only_so_many_documents_are_remembered_and_the_oldest_go() {
        let mut state = State::default();
        for index in 0..MAX_DOCUMENTS + 10 {
            let path = PathBuf::from(format!("/home/user/{index}.pdf"));
            state.remember_document(&path, view(0, Zoom::FitWidth, index as i64));
        }

        assert_eq!(state.document_count(), MAX_DOCUMENTS);
        assert!(state.document(Path::new("/home/user/0.pdf")).is_none());
        assert!(state.document(Path::new("/home/user/9.pdf")).is_none());
        assert!(state.document(Path::new("/home/user/10.pdf")).is_some());

        let read = State::parse(&state.serialize());
        assert_eq!(read.document_count(), MAX_DOCUMENTS);
    }

    /// A file holding more than the limit — written by hand, or by a
    /// version with a higher one — is cut down on the way in.
    #[test]
    fn an_overlong_file_is_cut_to_the_limit_keeping_the_newest() {
        let text: String = (0..MAX_DOCUMENTS + 5)
            .map(|index| {
                format!("document\t{index}\t0\t0.0\tfit-width\tfile:///home/user/{index}.pdf\n")
            })
            .collect();

        let read = State::parse(&text);

        assert_eq!(read.document_count(), MAX_DOCUMENTS);
        assert!(read.document(Path::new("/home/user/4.pdf")).is_none());
        assert!(read.document(Path::new("/home/user/5.pdf")).is_some());
    }

    /// Two windows, one file: each takes in what the other wrote, and the
    /// newer record of a document both hold wins.
    #[test]
    fn merging_keeps_the_other_window_s_documents_and_the_newer_of_a_shared_one() {
        let mut mine = State::default();
        mine.remember_document(
            Path::new("/home/user/shared.pdf"),
            view(1, Zoom::FitWidth, 10),
        );
        mine.remember_document(
            Path::new("/home/user/mine.pdf"),
            view(2, Zoom::FitWidth, 40),
        );
        let mut theirs = State::default();
        theirs.remember_document(
            Path::new("/home/user/shared.pdf"),
            view(7, Zoom::FitPage, 30),
        );
        theirs.remember_document(
            Path::new("/home/user/theirs.pdf"),
            view(3, Zoom::FitWidth, 20),
        );

        mine.merge_documents(&theirs);

        assert_eq!(mine.document_count(), 3);
        let shared = mine
            .document(Path::new("/home/user/shared.pdf"))
            .expect("shared");
        assert_eq!((shared.seen, shared.reading.page_index), (30, 7));
        assert!(mine.document(Path::new("/home/user/theirs.pdf")).is_some());
        let order: Vec<String> = mine
            .serialize()
            .lines()
            .skip(1)
            .map(|line| line.rsplit('/').next().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(order, ["mine.pdf", "shared.pdf", "theirs.pdf"]);
    }

    #[test]
    fn merging_an_older_record_of_a_document_changes_nothing() {
        let mut mine = sample();
        let mut theirs = State::default();
        theirs.remember_document(Path::new("/home/user/b.pdf"), view(8, Zoom::FitPage, 1));

        mine.merge_documents(&theirs);

        assert_eq!(
            mine.document(Path::new("/home/user/b.pdf"))
                .expect("b.pdf")
                .reading
                .page_index,
            0
        );
    }

    fn reading(page_index: usize, fraction: f64) -> ReadingPosition {
        ReadingPosition {
            page_index,
            fraction,
        }
    }

    #[test]
    fn a_position_on_a_page_that_is_still_there_is_kept() {
        assert_eq!(clamp_reading(reading(3, 0.4), 10), Some(reading(3, 0.4)));
        assert_eq!(clamp_reading(reading(9, 1.0), 10), Some(reading(9, 1.0)));
    }

    /// The document was replaced by a shorter one: the top of its last
    /// page, not the same fraction of some other page.
    #[test]
    fn a_position_past_the_end_goes_to_the_top_of_the_last_page() {
        assert_eq!(clamp_reading(reading(40, 0.7), 10), Some(reading(9, 0.0)));
        assert_eq!(clamp_reading(reading(10, 0.0), 10), Some(reading(9, 0.0)));
        assert_eq!(clamp_reading(reading(1, 0.5), 1), Some(reading(0, 0.0)));
    }

    #[test]
    fn a_document_with_no_pages_has_no_position() {
        assert_eq!(clamp_reading(reading(0, 0.0), 0), None);
    }

    #[test]
    fn a_fraction_that_is_out_of_range_is_brought_back_into_it() {
        assert_eq!(clamp_reading(reading(2, 4.0), 5), Some(reading(2, 1.0)));
        assert_eq!(clamp_reading(reading(2, -1.0), 5), Some(reading(2, 0.0)));
        assert_eq!(
            clamp_reading(reading(2, f64::NAN), 5),
            Some(reading(2, 0.0))
        );
    }
}
