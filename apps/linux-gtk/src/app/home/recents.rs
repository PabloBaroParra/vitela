//! Home's "Recent" list: the documents this desktop opened last, grouped by
//! day, each with a rendered first page.
//!
//! ## Where the list comes from
//!
//! `GtkRecentManager` — the freedesktop recent-files store every GTK app
//! shares — rather than a private file of our own. Three reasons, in order of
//! weight: a PDF opened in the file manager or another viewer is genuinely
//! recent to the user and shows up here without this shell having to have
//! been the one to open it; the store already handles its own eviction,
//! locking and cross-process updates, none of which is worth reimplementing;
//! and removing the application removes nothing the user has to clean up.
//!
//! The cost is that the store is not ours to trust: entries survive the files
//! they point at, cover every application, and carry whatever MIME type
//! whoever registered them supplied. [`recent_pdfs`] filters on all three
//! counts before anything reaches the screen.
//!
//! ## Why the previews are rendered here rather than read from a cache
//!
//! The freedesktop thumbnail spec would let us read `~/.cache/thumbnails`,
//! but only if some other program had already written a thumbnail for that
//! exact file — which for a PDF usually means a file manager that may never
//! have visited the folder. Rendering page 1 ourselves is one pdfium open per
//! card, at [`Priority::Thumbnail`], off the UI thread, and always correct.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    gdk_pixbuf, gio, glib, Align, Box as GtkBox, Button, FlowBox, Label, Orientation, Picture,
    RecentManager, SelectionMode,
};
use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError, RenderOptions};

use crate::app::document::open_file;
use crate::app::render::render_result;
use crate::app::state::{RenderedPage, Viewer};

use super::is_pdf_path;

/// How many documents the list shows. The recent store holds far more; this
/// is a launch screen, not a file manager, and every extra card is another
/// pdfium open before the page settles.
const MAX_RECENTS: usize = 8;

/// Logical size of a card's preview. Portrait, because A4 and Letter both
/// are, so a landscape page letterboxes rather than the common case cropping.
/// Logical: [`thumbnail_dpi`] multiplies by the monitor's scale factor, the
/// same way `render` sizes the real page bitmaps.
const THUMB_WIDTH_PX: i32 = 108;
const THUMB_HEIGHT_PX: i32 = 140;

const POINTS_PER_INCH: f64 = 72.0;

/// Which day-group a document falls in. Ordered as the groups are stacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bucket {
    Today,
    Yesterday,
    Earlier,
}

impl Bucket {
    const ALL: [Bucket; 3] = [Bucket::Today, Bucket::Yesterday, Bucket::Earlier];

    fn chip(self) -> &'static str {
        match self {
            Bucket::Today => "Today",
            Bucket::Yesterday => "Yesterday",
            Bucket::Earlier => "Earlier",
        }
    }
}

/// One entry of the recent store, already reduced to what a card needs.
///
/// Constructed from `GtkRecentInfo` by [`recent_pdfs`] in production, and
/// directly by the tests — which is the point of it being a plain value type:
/// the list can be exercised without writing to the user's real recent store.
#[derive(Clone, Debug)]
pub(crate) struct RecentPdf {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) visited: glib::DateTime,
}

/// A card and the facts the list needs about it after construction.
struct Card {
    /// Lowercased once at build time — the filter runs on every keystroke.
    search_key: String,
    button: Button,
    thumb: Picture,
    meta: Label,
    /// The "Opened today, 10:45" half of the meta line, kept so the page
    /// count can be appended to it when the preview finishes without having
    /// to re-derive the time.
    opened: String,
    path: PathBuf,
}

/// One day-group: its chip, the flow of cards under it, and those cards.
struct Group {
    container: GtkBox,
    cards: Vec<Card>,
}

#[derive(Clone)]
pub(crate) struct RecentsSection {
    pub(crate) root: GtkBox,
    /// Holds the day-groups. Cleared and refilled by [`RecentsSection::rebuild_from`]
    /// — the section widget itself outlives every rebuild so the page layout
    /// around it never moves.
    body: GtkBox,
    groups: Rc<RefCell<Vec<Group>>>,
    /// The header's current search text, lowercased.
    ///
    /// Held here rather than read back off the entry so a rebuild can re-apply
    /// it: `GtkRecentManager::changed` fires for any application's addition,
    /// including our own on every open, and a rebuild that ignored the filter
    /// would repopulate the list unfiltered while the search box still showed
    /// what it was filtered by.
    query: Rc<RefCell<String>>,
    empty: Label,
    /// Bumped by every rebuild. The preview task captures the value it
    /// started with and stops as soon as it no longer matches, so previews
    /// from a superseded list can never paint into the current one.
    generation: Rc<Cell<u64>>,
}

/// Builds the section and fills it from the recent store, then keeps it in
/// step: `GtkRecentManager::changed` fires for *any* application's addition,
/// so a PDF opened elsewhere while this window is up appears without the user
/// having to do anything.
pub(crate) fn build_recents_section(viewer: &Viewer) -> RecentsSection {
    let heading = Label::new(Some("Recent"));
    heading.set_xalign(0.0);
    heading.set_hexpand(true);
    heading.add_css_class("home-section-title");

    let heading_row = GtkBox::new(Orientation::Horizontal, 8);
    heading_row.append(&heading);

    let body = GtkBox::new(Orientation::Vertical, 14);

    let empty = Label::new(None);
    empty.set_xalign(0.0);
    empty.set_wrap(true);
    empty.add_css_class("home-empty");

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.append(&heading_row);
    root.append(&body);
    root.append(&empty);

    let section = RecentsSection {
        root,
        body,
        groups: Rc::new(RefCell::new(Vec::new())),
        query: Rc::new(RefCell::new(String::new())),
        empty,
        generation: Rc::new(Cell::new(0)),
    };
    section.rebuild(viewer);

    RecentManager::default().connect_changed({
        let section = section.clone();
        let viewer = viewer.clone();
        move |_| section.rebuild(&viewer)
    });

    section
}

impl RecentsSection {
    fn rebuild(&self, viewer: &Viewer) {
        self.rebuild_from(recent_pdfs(MAX_RECENTS), viewer);
    }

    /// Replaces the list with `entries`. Separated from [`Self::rebuild`] so
    /// the tests can drive it with entries they own rather than whatever the
    /// machine running them happens to have opened lately.
    fn rebuild_from(&self, entries: Vec<RecentPdf>, viewer: &Viewer) {
        self.generation.set(self.generation.get() + 1);
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }

        let now = glib::DateTime::now_local().unwrap_or_else(|_| {
            // `now_local` fails only if the timezone database is unreadable.
            // UTC still buckets and formats correctly, just in the wrong
            // offset — far better than dropping the list.
            glib::DateTime::now_utc().expect("a clock reading must be available")
        });

        let mut groups = Vec::new();
        for bucket in Bucket::ALL {
            let matching: Vec<_> = entries
                .iter()
                .filter(|entry| bucket_of(&entry.visited, &now) == bucket)
                .collect();
            if matching.is_empty() {
                continue;
            }
            let flow = FlowBox::new();
            flow.set_selection_mode(SelectionMode::None);
            flow.set_homogeneous(false);
            flow.set_row_spacing(10);
            flow.set_column_spacing(10);

            let chip = Label::new(Some(bucket.chip()));
            chip.set_xalign(0.0);
            chip.set_halign(Align::Start);
            chip.add_css_class("day-chip");

            let container = GtkBox::new(Orientation::Vertical, 8);
            container.append(&chip);
            container.append(&flow);
            self.body.append(&container);

            let cards = matching
                .into_iter()
                .map(|entry| build_card(&flow, entry, &now, viewer))
                .collect();
            groups.push(Group { container, cards });
        }
        *self.groups.borrow_mut() = groups;

        let query = self.query.borrow().clone();
        self.apply_visibility(entries.is_empty(), |card| card.search_key.contains(&query));
        self.spawn_previews();
    }

    /// Puts the keyboard on the newest card still on screen, if there is one.
    ///
    /// What the app rail's Recent button does after switching to Home:
    /// focusing a real card both answers "where was I" and lets the list be
    /// walked with the arrow keys — and GTK scrolls the focused widget into
    /// view, so the list comes to the user rather than the other way round.
    pub(crate) fn focus_first_card(&self) -> bool {
        self.groups
            .borrow()
            .iter()
            .flat_map(|group| &group.cards)
            .find(|card| card.button.is_visible())
            .map(|card| card.button.grab_focus())
            .unwrap_or(false)
    }

    /// Hides every card whose filename does not contain `query`, then hides
    /// any day-group left with nothing in it.
    pub(crate) fn filter(&self, query: &str) {
        query.clone_into(&mut self.query.borrow_mut());
        let nothing_at_all = self
            .groups
            .borrow()
            .iter()
            .all(|group| group.cards.is_empty());
        self.apply_visibility(nothing_at_all, |card| card.search_key.contains(query));
    }

    fn apply_visibility(&self, nothing_at_all: bool, keep: impl Fn(&Card) -> bool) {
        let mut shown = 0;
        for group in self.groups.borrow().iter() {
            let mut group_shown = 0;
            for card in &group.cards {
                let visible = keep(card);
                card.button.set_visible(visible);
                group_shown += usize::from(visible);
            }
            group.container.set_visible(group_shown > 0);
            shown += group_shown;
        }
        self.empty.set_visible(shown == 0);
        self.empty.set_text(if nothing_at_all {
            "No recent documents yet. Open a PDF and it will show up here."
        } else {
            "No recent document matches that search."
        });
    }

    /// Renders each visible card's first page, one document at a time.
    ///
    /// Sequential rather than a task per card: pdfium is behind a single
    /// actor, so eight concurrent opens would queue there anyway — but they
    /// would queue *ahead of* whatever the user opens next, because each
    /// would already have been submitted. Awaiting one at a time leaves the
    /// queue free between cards.
    fn spawn_previews(&self) {
        let generation = self.generation.get();
        let generation_cell = self.generation.clone();
        let targets: Vec<(PathBuf, Picture, Label, String)> = self
            .groups
            .borrow()
            .iter()
            .flat_map(|group| &group.cards)
            .map(|card| {
                (
                    card.path.clone(),
                    card.thumb.clone(),
                    card.meta.clone(),
                    card.opened.clone(),
                )
            })
            .collect();

        glib::spawn_future_local(async move {
            for (path, thumb, meta, opened) in targets {
                if generation_cell.get() != generation {
                    return;
                }
                let job_path = path.clone();
                // Read here rather than inside the blocking job: it is a
                // widget property, and the job runs off the main thread.
                let scale_factor = thumb.scale_factor().max(1);
                let Ok(Ok(preview)) =
                    gio::spawn_blocking(move || render_preview(&job_path, scale_factor)).await
                else {
                    // A preview that cannot be produced — an encrypted file,
                    // a deleted one, a broken one — leaves the placeholder
                    // frame and the plain "Opened …" line. It is never worth
                    // a status-bar message: the user did not ask for it.
                    continue;
                };
                if generation_cell.get() != generation {
                    return;
                }
                apply_preview(&thumb, &meta, &opened, preview);
            }
        });
    }
}

fn build_card(flow: &FlowBox, entry: &RecentPdf, now: &glib::DateTime, viewer: &Viewer) -> Card {
    let thumb = Picture::new();
    thumb.add_css_class("recent-thumb");
    thumb.set_can_shrink(true);
    thumb.set_content_fit(gtk::ContentFit::Contain);
    thumb.set_size_request(THUMB_WIDTH_PX, THUMB_HEIGHT_PX);

    let name = Label::new(Some(&entry.name));
    name.set_xalign(0.0);
    name.add_css_class("recent-name");
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    // The middle of a filename is the part that varies; capping the request
    // here is what stops one long name from setting the whole flow's column
    // width, the same reason `tools_panel::property_row` caps its value.
    name.set_max_width_chars(14);

    let opened = opened_text(&entry.visited, now);
    let meta = Label::new(Some(&opened));
    meta.set_xalign(0.0);
    meta.add_css_class("recent-meta");

    let content = GtkBox::new(Orientation::Vertical, 6);
    content.append(&thumb);
    content.append(&name);
    content.append(&meta);

    let button = Button::new();
    button.set_child(Some(&content));
    button.add_css_class("recent-card");
    button.set_tooltip_text(Some(&entry.path.to_string_lossy()));
    button.update_property(&[gtk::accessible::Property::Label(&format!(
        "Open {}",
        entry.name
    ))]);
    button.connect_clicked({
        let viewer = viewer.clone();
        let path = entry.path.clone();
        move |_| open_file(&viewer, path.clone())
    });
    flow.append(&button);

    Card {
        search_key: entry.name.to_lowercase(),
        button,
        thumb,
        meta,
        opened,
        path: entry.path.clone(),
    }
}

/// Registers `path` with the desktop's recent store, so it appears in this
/// list — and in every other application's — next time.
///
/// Called from `document.rs` once an open has actually succeeded. Registering
/// on *attempt* would fill the list with files that turned out unreadable.
pub(crate) fn remember(path: &Path) {
    RecentManager::default().add_item(&gio::File::for_path(path).uri());
}

/// The recent store's PDF entries, newest first, capped at `limit`.
fn recent_pdfs(limit: usize) -> Vec<RecentPdf> {
    let mut entries: Vec<RecentPdf> = RecentManager::default()
        .items()
        .into_iter()
        // Remote entries (`sftp://`, `smb://`) have no path pdfium can open,
        // and this shell has no download step to give them one.
        .filter(|info| info.is_local())
        .filter_map(|info| {
            let path = gio::File::for_uri(&info.uri()).path()?;
            let looks_like_pdf = info.mime_type() == "application/pdf" || is_pdf_path(&path);
            // `exists` rather than trusting the store: entries outlive the
            // files they point at, and a card that cannot open is worse than
            // no card.
            if !looks_like_pdf || !path.exists() {
                return None;
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| info.display_name().to_string());
            Some(RecentPdf {
                path,
                name,
                visited: info.visited(),
            })
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.visited.to_unix()));
    entries.truncate(limit);
    entries
}

/// Which day-group `visited` belongs to, relative to `now`.
///
/// Calendar days, not elapsed hours: something opened at 23:50 yesterday is
/// "Yesterday" at 00:10 today, which is how the user remembers it — an
/// elapsed-time rule would call it "Today".
fn bucket_of(visited: &glib::DateTime, now: &glib::DateTime) -> Bucket {
    if same_day(visited, now) {
        return Bucket::Today;
    }
    let yesterday = now.add_days(-1);
    match yesterday {
        Ok(yesterday) if same_day(visited, &yesterday) => Bucket::Yesterday,
        _ => Bucket::Earlier,
    }
}

fn same_day(left: &glib::DateTime, right: &glib::DateTime) -> bool {
    left.year() == right.year() && left.day_of_year() == right.day_of_year()
}

/// The card's "Opened …" line.
fn opened_text(visited: &glib::DateTime, now: &glib::DateTime) -> String {
    let format = |pattern: &str| {
        visited
            .format(pattern)
            .map(|text| text.to_string())
            .unwrap_or_default()
    };
    match bucket_of(visited, now) {
        Bucket::Today => format!("Opened today, {}", format("%H:%M")),
        Bucket::Yesterday => format!("Opened yesterday, {}", format("%H:%M")),
        Bucket::Earlier => format!("Opened {}", format("%e %b")).replace("  ", " "),
    }
}

/// `"1 page"` / `"12 pages"`.
fn page_count_text(pages: u32) -> String {
    if pages == 1 {
        "1 page".to_string()
    } else {
        format!("{pages} pages")
    }
}

struct Preview {
    /// `None` for a document with no pages — it still has a page *count*
    /// worth showing, and there is nothing to rasterise.
    page: Option<RenderedPage>,
    page_count: u32,
}

/// Opens `path`, renders its first page small, and closes it again.
///
/// Runs on a blocking task, never the UI thread. No password is supplied, so
/// an encrypted document fails here rather than raising a prompt the user
/// never asked for — the card simply keeps its placeholder.
fn render_preview(path: &Path, scale_factor: i32) -> Result<Preview, RenderError> {
    let renderer = PdfiumRenderer::new();
    let document = renderer.open_document(path, None)?;
    let preview = preview_of(&renderer, document, scale_factor);
    let _ = renderer.close_document(document);
    preview
}

fn preview_of(
    renderer: &PdfiumRenderer,
    document: DocumentHandle,
    scale_factor: i32,
) -> Result<Preview, RenderError> {
    let page_count = renderer.page_count(document, Priority::Thumbnail).wait()?;
    if page_count == 0 {
        return Ok(Preview {
            page: None,
            page_count,
        });
    }
    let (width_pt, height_pt) = renderer
        .page_size(document, 0, Priority::Thumbnail)
        .wait()?;
    let page = render_result(renderer.render_page(
        document,
        0,
        thumbnail_dpi(width_pt, height_pt, scale_factor),
        None,
        RenderOptions::new(),
        Priority::Thumbnail,
    ))?;
    Ok(Preview {
        page: Some(page),
        page_count,
    })
}

/// The DPI that fits a `width_pt` x `height_pt` page inside the card's frame
/// at `scale_factor` device pixels per logical pixel.
///
/// The smaller of the two fits, so the whole page lands inside the frame and
/// `ContentFit::Contain` letterboxes the spare axis — a landscape page shows
/// as a wide strip rather than being cropped to portrait.
///
/// The scale factor is not a detail: the frame is stated in *logical* pixels,
/// so on a HiDPI monitor a bitmap rasterised for the logical size is half the
/// resolution the card is drawn at, and every preview is visibly soft.
fn thumbnail_dpi(width_pt: f32, height_pt: f32, scale_factor: i32) -> u32 {
    let scale = f64::from(scale_factor.max(1));
    let fit = |pixels: i32, points: f32| {
        f64::from(pixels) * scale * POINTS_PER_INCH / f64::from(points.max(1.0))
    };
    // Floor, not round: rounding up overshoots the frame by a pixel or two,
    // which `ContentFit::Contain` then scales back down — a resample of a
    // bitmap that was correct to begin with, for no gain.
    fit(THUMB_WIDTH_PX, width_pt)
        .min(fit(THUMB_HEIGHT_PX, height_pt))
        .floor()
        .clamp(8.0, 300.0) as u32
}

fn apply_preview(thumb: &Picture, meta: &Label, opened: &str, preview: Preview) {
    if let Some(page) = preview.page {
        let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
            &glib::Bytes::from_owned(page.pixels),
            gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            page.width as i32,
            page.height as i32,
            page.stride as i32,
        );
        thumb.set_pixbuf(Some(&pixbuf));
    }
    meta.set_text(&format!(
        "{opened} · {}",
        page_count_text(preview.page_count)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ui_tests::built_ui;

    fn at(year: i32, month: i32, day: i32, hour: i32) -> glib::DateTime {
        glib::DateTime::new(&glib::TimeZone::utc(), year, month, day, hour, 0, 0.0)
            .expect("the fixture timestamp must be valid")
    }

    fn entry(name: &str, visited: glib::DateTime) -> RecentPdf {
        RecentPdf {
            path: PathBuf::from(format!("/tmp/{name}")),
            name: name.to_string(),
            visited,
        }
    }

    /// Calendar days, not elapsed hours — the reason `bucket_of` compares
    /// dates instead of subtracting timestamps. Ten past midnight, something
    /// opened at ten to midnight is *yesterday*, twenty minutes earlier.
    #[test]
    fn buckets_are_calendar_days_not_elapsed_hours() {
        let now = at(2026, 9, 1, 0);
        assert_eq!(bucket_of(&at(2026, 9, 1, 0), &now), Bucket::Today);
        assert_eq!(bucket_of(&at(2026, 8, 31, 23), &now), Bucket::Yesterday);
        assert_eq!(bucket_of(&at(2026, 8, 30, 23), &now), Bucket::Earlier);
    }

    /// A year boundary is the case a naive `day_of_year` comparison gets
    /// wrong: 1 January and 1 January a year earlier share a day number.
    #[test]
    fn buckets_do_not_confuse_the_same_day_of_a_different_year() {
        let now = at(2026, 1, 1, 12);
        assert_eq!(bucket_of(&at(2025, 1, 1, 12), &now), Bucket::Earlier);
        assert_eq!(bucket_of(&at(2025, 12, 31, 12), &now), Bucket::Yesterday);
    }

    #[test]
    fn page_counts_read_as_english() {
        assert_eq!(page_count_text(1), "1 page");
        assert_eq!(page_count_text(12), "12 pages");
        assert_eq!(page_count_text(0), "0 pages");
    }

    /// The whole point of taking the *smaller* fit: a landscape page must
    /// come back at the DPI that fits its width, not one that fits its height
    /// and overflows the card.
    /// Pixels a page of `points` comes out at when rendered at `dpi`.
    fn rendered_px(points: f32, dpi: u32) -> f64 {
        (f64::from(dpi) * f64::from(points) / POINTS_PER_INCH).round()
    }

    #[test]
    fn thumbnail_dpi_fits_the_page_inside_the_frame() {
        let a4 = thumbnail_dpi(595.0, 842.0, 1);
        assert!(rendered_px(842.0, a4) <= f64::from(THUMB_HEIGHT_PX));
        assert!(rendered_px(595.0, a4) <= f64::from(THUMB_WIDTH_PX));

        let landscape = thumbnail_dpi(842.0, 595.0, 1);
        assert!(rendered_px(842.0, landscape) <= f64::from(THUMB_WIDTH_PX));
    }

    /// The frame is stated in logical pixels, so a 2x monitor has to be
    /// rasterised for at twice the resolution or every preview is soft.
    ///
    /// Not *exactly* double: the DPI floors so the bitmap never overshoots
    /// its frame, and flooring a target twice as large loses a different
    /// fraction. Within a DPI of double, and still inside the frame, is the
    /// property that matters.
    #[test]
    fn thumbnail_dpi_scales_up_for_a_hidpi_monitor() {
        let one_to_one = thumbnail_dpi(595.0, 842.0, 1);
        let retina = thumbnail_dpi(595.0, 842.0, 2);

        assert!(
            retina >= one_to_one * 2 - 1,
            "{retina} dpi is not twice the {one_to_one} a 1x monitor gets"
        );
        assert!(rendered_px(842.0, retina) <= f64::from(THUMB_HEIGHT_PX * 2));
        assert!(rendered_px(595.0, retina) <= f64::from(THUMB_WIDTH_PX * 2));
    }

    /// A degenerate page size, or a nonsense scale factor, must not divide by
    /// zero or ask pdfium for an impossible DPI.
    #[test]
    fn thumbnail_dpi_survives_a_zero_sized_page_and_a_zero_scale() {
        assert!((8..=300).contains(&thumbnail_dpi(0.0, 0.0, 1)));
        assert!((8..=300).contains(&thumbnail_dpi(595.0, 842.0, 0)));
    }

    #[gtk::test]
    fn gtk_ui_recents_group_by_day_and_hide_the_groups_they_do_not_fill() {
        let built = built_ui();
        let section = build_recents_section(&built.viewer);
        let now = glib::DateTime::now_local().expect("a local clock must be available");

        section.rebuild_from(
            vec![
                entry("today.pdf", now.clone()),
                entry(
                    "older.pdf",
                    now.add_days(-9).expect("nine days back must be a date"),
                ),
            ],
            &built.viewer,
        );

        let chips: Vec<String> = section
            .groups
            .borrow()
            .iter()
            .filter_map(|group| group.container.first_child())
            .filter_map(|child| child.downcast::<Label>().ok())
            .map(|chip| chip.text().to_string())
            .collect();
        assert_eq!(chips, vec!["Today".to_string(), "Earlier".to_string()]);

        built.window.close();
    }

    /// The search entry filters by filename, and a group whose every card is
    /// filtered out goes with them rather than leaving a chip over a gap.
    #[gtk::test]
    fn gtk_ui_filtering_hides_non_matching_cards_and_their_empty_groups() {
        let built = built_ui();
        let section = build_recents_section(&built.viewer);
        let now = glib::DateTime::now_local().expect("a local clock must be available");

        section.rebuild_from(
            vec![
                entry("invoice.pdf", now.clone()),
                entry(
                    "contract.pdf",
                    now.add_days(-4).expect("four days back must be a date"),
                ),
            ],
            &built.viewer,
        );
        section.filter("invoice");

        let groups = section.groups.borrow();
        assert!(groups[0].container.is_visible());
        assert!(groups[0].cards[0].button.is_visible());
        assert!(
            !groups[1].container.is_visible(),
            "a group with no matching card must not leave its chip on screen"
        );
        assert!(!section.empty.is_visible());
        drop(groups);

        section.filter("nothing-matches-this");
        assert!(section.empty.is_visible());
        assert_eq!(
            section.empty.text(),
            "No recent document matches that search."
        );

        built.window.close();
    }

    /// An empty store is a first-run state, not a failed search, and says so.
    #[gtk::test]
    fn gtk_ui_an_empty_list_explains_itself() {
        let built = built_ui();
        let section = build_recents_section(&built.viewer);

        section.rebuild_from(Vec::new(), &built.viewer);

        assert!(section.empty.is_visible());
        assert_eq!(
            section.empty.text(),
            "No recent documents yet. Open a PDF and it will show up here."
        );

        built.window.close();
    }
}
