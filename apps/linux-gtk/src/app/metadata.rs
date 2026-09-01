//! Document properties panel (T-176, Batch 22 UI — see
//! `docs/batch-metadata-edit.md`): editable Title/Author/Subject/Keywords/
//! Creator/Producer, plus a Creation/Mod date each, recorded as one
//! `Command::SetDocumentInfo` per edit (batch decision 5 — the `/Info` dict
//! is edited as a unit from a single panel, not one command per field, the
//! same posture `forms::style::restyle_selected` takes for a field's style).
//!
//! **Reading the current value.** `Document` never mirrors `DocumentInfo`
//! itself — `SetDocumentInfo::apply` is inert, same as every Batch 21
//! page-content command, because `pdf-save` replays the log's `after` into
//! `/Info` at write time rather than keeping a live copy in the model (see
//! `edit_log.rs`'s own doc for `SetDocumentInfo`). So "the current value"
//! here means: the last `SetDocumentInfo.after` still in `pending_edits`, or
//! — if this session has never recorded one — `LopdfDocument::document_info`'s
//! lazy read of the file's `/Info` dict as it was at open time. This is the
//! same "replay pending log over a lazy base read" shape `content_edit::model`
//! already uses for page content; metadata just does not need per-page
//! caching (there is exactly one dict, not one per page).
//!
//! **Permission gate.** Gated on [`Viewer::content_edit_refusal`] — `/P` bit
//! 4, "modify the contents of the document by operations other than those
//! controlled by" the narrower bits (`pdf-manip::security`'s own doc).
//! `/Info` is not page content, but it is not covered by any of the narrower
//! bits either (annotate, copy/extract, print, assembly), so the general
//! modify-contents permission is the closest fit — same judgment call this
//! shell already made for T-161's text-run/image edits.
//!
//! **Text fields vs. date fields.** A text field (`Title`, `Author`, ...)
//! commits on every keystroke, no different from `forms::fill`'s `Entry`
//! controls — any string is a valid value, so there is nothing to validate
//! and no reason to make the user find a separate "apply" action. A date
//! field is different: not every string is a valid date, so it commits only
//! on Enter or on losing focus, and either an invalid or an accepted edit is
//! followed by [`refresh`] — invalid, to bounce the field back to the last
//! real value; accepted, to show the normalized `YYYY-MM-DD HH:MM:SS` even if
//! the user typed a shorter form (a bare `2026-09-01` is accepted, time
//! defaulting to midnight).
//!
//! **Display format, not [`PdfDate::to_pdf_string`].** The raw
//! `D:YYYYMMDDHHmmSSOHH'mm'` the file (and `PdfDate`'s own `Display`) uses is
//! not something to make a user type by hand, so the two date fields show and
//! parse a friendlier `YYYY-MM-DD HH:MM:SS` instead ([`friendly_display`]/
//! [`parse_friendly_date`], local to this module — `PdfDate` itself is
//! unchanged). That format has no UT-offset component of its own, so the
//! offset an existing date already carried has nowhere to round-trip through
//! the `Entry`'s text; [`MetadataPanel::creation_offset`]/`mod_offset` hold it
//! on the side instead, updated whenever [`set`] reads a real value and
//! consulted by [`commit_date`] when building the edited `PdfDate` back up —
//! so editing a date's day or hour does not silently reset a document whose
//! original offset was `+05'30'` to UTC. There is no UI for editing the
//! offset itself in v1; a field with no prior date defaults it to
//! [`PdfDateOffset::Utc`], the same default `PdfDate::parse` falls back to
//! for a wholly absent offset.
//!
//! **Future dates are allowed on purpose.** Neither `PdfDate::parse` nor this
//! module checks a typed date against the system clock, or does calendar
//! validation beyond each component's own range (`PdfDate`'s own doc already
//! notes `2026-02-30` round-trips like any other well-formed value). `/Info`
//! is a metadata claim, not a verified fact — real viewers (Acrobat) accept
//! any date here too, and batch decision 9 already commits this feature to
//! staying "purely user-editable" with no auto-stamp cleverness layered on
//! top. A future `CreationDate` is exactly as valid as a past one.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Entry, Orientation};
use pdf_document::{Command, Document, DocumentInfo, PdfDate, PdfDateOffset};

use crate::app::state::{
    DocumentSession, MetadataPanel, SaveBacking, Viewer, CONTENT_MODEL_UNAVAILABLE,
};

use super::tools_panel::{panel_heading, property_row, EMPTY};

const NO_DOCUMENT: &str = "Open a PDF before editing its properties.";

/// Which of the two date fields a [`commit_date`] call is targeting.
#[derive(Clone, Copy)]
enum DateField {
    Creation,
    Mod,
}

/// Builds the panel's widgets — no signal wiring yet, so this needs no
/// `&Viewer` (which does not exist at this point in `build_ui`: this runs
/// before the `Viewer` struct literal that will go on to hold the very panel
/// this returns). [`connect_metadata_panel`] wires the commit handlers once
/// `Viewer` exists — mirrors `forms::build_forms_content`/
/// `forms::connect_forms_toolbar`'s own build/connect split.
///
/// Returns the panel (for `Viewer::metadata`) and its container (for
/// `tools_panel::build_tools_panel` to place).
pub(crate) fn build_metadata_panel() -> (MetadataPanel, GtkBox) {
    let container = GtkBox::new(Orientation::Vertical, 6);
    container.append(&panel_heading("Document properties"));

    let pages = property_row(&container, "Pages");
    let title = entry_row(&container, "Title");
    let author = entry_row(&container, "Author");
    let subject = entry_row(&container, "Subject");
    let keywords = entry_row(&container, "Keywords");
    let creator = entry_row(&container, "Creator");
    let producer = entry_row(&container, "Producer");
    let creation_date = entry_row(&container, "Created");
    let mod_date = entry_row(&container, "Modified");
    for date_entry in [&creation_date, &mod_date] {
        date_entry.set_placeholder_text(Some("YYYY-MM-DD HH:MM:SS"));
    }

    let panel = MetadataPanel {
        syncing: Rc::new(Cell::new(false)),
        pages,
        title,
        author,
        subject,
        keywords,
        creator,
        producer,
        creation_date,
        mod_date,
        creation_offset: Rc::new(Cell::new(PdfDateOffset::Utc)),
        mod_offset: Rc::new(Cell::new(PdfDateOffset::Utc)),
    };
    set_empty(&panel);
    (panel, container)
}

/// Wires every field's commit handler — the metadata twin of
/// `forms::connect_forms_toolbar`. Called once from `build_ui`, right after
/// the `Viewer` struct (and so `viewer.metadata`) exists.
pub(crate) fn connect_metadata_panel(viewer: &Viewer) {
    wire_text_entry(viewer, &viewer.metadata.title, |info, text| {
        info.title = text
    });
    wire_text_entry(viewer, &viewer.metadata.author, |info, text| {
        info.author = text
    });
    wire_text_entry(viewer, &viewer.metadata.subject, |info, text| {
        info.subject = text
    });
    wire_text_entry(viewer, &viewer.metadata.keywords, |info, text| {
        info.keywords = text
    });
    wire_text_entry(viewer, &viewer.metadata.creator, |info, text| {
        info.creator = text
    });
    wire_text_entry(viewer, &viewer.metadata.producer, |info, text| {
        info.producer = text
    });
    wire_date_entry(viewer, &viewer.metadata.creation_date, DateField::Creation);
    wire_date_entry(viewer, &viewer.metadata.mod_date, DateField::Mod);
}

/// One "key: [editable value]" row — the editable twin of
/// `tools_panel::property_row`.
fn entry_row(container: &GtkBox, key: &str) -> Entry {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.add_css_class("property-row");

    let key_label = gtk::Label::new(Some(key));
    key_label.set_xalign(0.0);
    key_label.set_width_chars(8);
    key_label.add_css_class("property-key");

    let entry = Entry::new();
    entry.set_hexpand(true);
    entry.add_css_class("property-value");

    row.append(&key_label);
    row.append(&entry);
    container.append(&row);
    entry
}

fn set_empty(panel: &MetadataPanel) {
    panel.syncing.set(true);
    panel.pages.set_text(EMPTY);
    for entry in text_entries(panel) {
        entry.set_text("");
    }
    panel.creation_offset.set(PdfDateOffset::Utc);
    panel.mod_offset.set(PdfDateOffset::Utc);
    panel.syncing.set(false);
    set_sensitive(panel, false);
}

fn text_entries(panel: &MetadataPanel) -> [&Entry; 8] {
    [
        &panel.title,
        &panel.author,
        &panel.subject,
        &panel.keywords,
        &panel.creator,
        &panel.producer,
        &panel.creation_date,
        &panel.mod_date,
    ]
}

fn set_sensitive(panel: &MetadataPanel, enabled: bool) {
    for entry in text_entries(panel) {
        entry.set_sensitive(enabled);
    }
}

/// Fills every widget from `page_count`/`info`, under the `syncing` guard so
/// the entries' own commit handlers do not mistake this write-back for a
/// user edit.
fn set(panel: &MetadataPanel, page_count: usize, info: &DocumentInfo) {
    panel.syncing.set(true);
    panel.pages.set_text(&page_count.to_string());
    panel.title.set_text(info.title.as_deref().unwrap_or(""));
    panel.author.set_text(info.author.as_deref().unwrap_or(""));
    panel
        .subject
        .set_text(info.subject.as_deref().unwrap_or(""));
    panel
        .keywords
        .set_text(info.keywords.as_deref().unwrap_or(""));
    panel
        .creator
        .set_text(info.creator.as_deref().unwrap_or(""));
    panel
        .producer
        .set_text(info.producer.as_deref().unwrap_or(""));
    panel
        .creation_date
        .set_text(&friendly_display(info.creation_date));
    panel.mod_date.set_text(&friendly_display(info.mod_date));
    panel.creation_offset.set(
        info.creation_date
            .map_or(PdfDateOffset::Utc, |date| date.offset),
    );
    panel
        .mod_offset
        .set(info.mod_date.map_or(PdfDateOffset::Utc, |date| date.offset));
    panel.syncing.set(false);
}

/// `PdfDate` -> `"YYYY-MM-DD HH:MM:SS"`, dropping the UT offset — see the
/// module doc for where that offset goes instead.
fn friendly_display(date: Option<PdfDate>) -> String {
    date.map(|date| {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            date.year, date.month, date.day, date.hour, date.minute, date.second
        )
    })
    .unwrap_or_default()
}

/// The date/time components [`parse_friendly_date`] extracts — no offset,
/// see the module doc for why that lives separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FriendlyDate {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

/// Parses the friendly display format back into date/time components (no
/// offset — the caller supplies that separately, see the module doc).
/// Accepts `"YYYY-MM-DD"`, `"YYYY-MM-DD HH:MM"`, or `"YYYY-MM-DD HH:MM:SS"` —
/// an omitted time defaults to midnight, an omitted seconds defaults to `0`,
/// the same "later components may be omitted" leniency `PdfDate::parse`
/// itself already extends to the stricter on-disk format. `Ok(None)` means an
/// all-whitespace/empty input, clearing the date.
fn parse_friendly_date(raw: &str) -> Result<Option<FriendlyDate>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let mut halves = trimmed.splitn(2, char::is_whitespace);
    let date_part = halves.next().unwrap_or_default();
    let time_part = halves.next().unwrap_or_default().trim();

    let mut date_fields = date_part.split('-');
    let mut next_date_field = |name: &'static str| -> Result<u32, String> {
        date_fields
            .next()
            .filter(|field| !field.is_empty())
            .ok_or_else(|| format!("expected YYYY-MM-DD, missing {name}"))?
            .parse::<u32>()
            .map_err(|_| format!("expected YYYY-MM-DD, \"{name}\" is not a number"))
    };
    let year = next_date_field("year")?;
    let month = next_date_field("month")?;
    let day = next_date_field("day")?;
    if date_fields.next().is_some() {
        return Err("expected YYYY-MM-DD, found extra \"-\"-separated parts".to_string());
    }
    let year = u16::try_from(year).map_err(|_| format!("year {year} does not fit"))?;
    let month = in_range(month, 1, 12, "month")?;
    let day = in_range(day, 1, 31, "day")?;

    let (hour, minute, second) = if time_part.is_empty() {
        (0, 0, 0)
    } else {
        let mut time_fields = time_part.split(':');
        let mut next_time_field = |name: &'static str| -> Result<u32, String> {
            time_fields
                .next()
                .filter(|field| !field.is_empty())
                .ok_or_else(|| format!("expected HH:MM or HH:MM:SS, missing {name}"))?
                .parse::<u32>()
                .map_err(|_| format!("expected HH:MM or HH:MM:SS, \"{name}\" is not a number"))
        };
        let hour = next_time_field("hour")?;
        let minute = next_time_field("minute")?;
        let second = match time_fields.next() {
            Some(field) => field.parse::<u32>().map_err(|_| {
                "expected HH:MM or HH:MM:SS, \"second\" is not a number".to_string()
            })?,
            None => 0,
        };
        if time_fields.next().is_some() {
            return Err(
                "expected HH:MM or HH:MM:SS, found extra \":\"-separated parts".to_string(),
            );
        }
        (
            in_range(hour, 0, 23, "hour")?,
            in_range(minute, 0, 59, "minute")?,
            in_range(second, 0, 59, "second")?,
        )
    };

    Ok(Some(FriendlyDate {
        year,
        month,
        day,
        hour,
        minute,
        second,
    }))
}

fn in_range(value: u32, min: u8, max: u8, name: &'static str) -> Result<u8, String> {
    if value < min as u32 || value > max as u32 {
        return Err(format!("{name} {value} is out of range ({min}-{max})"));
    }
    Ok(value as u8)
}

/// Rebuilds the panel from the current session — called after document
/// open/close, an undo/redo step, and every accepted or rejected date-field
/// commit (see the module doc). Never called from a text-field commit: every
/// text field is always valid, so there is nothing for a refresh to correct,
/// and calling `set_text` back onto the very `Entry` the user is mid-keystroke
/// in would reset its cursor to the end on every character typed.
pub(crate) fn refresh(viewer: &Viewer) {
    let state = viewer.state.borrow();
    let Some(session) = state.session.as_ref() else {
        drop(state);
        set_empty(&viewer.metadata);
        return;
    };
    let page_count = session.pages.len();
    let info = current_document_info(
        session.document_model.as_ref(),
        session.save_backing.as_ref(),
    );
    let enabled = session.content_edit_access.refusal().is_none();
    drop(state);
    set(&viewer.metadata, page_count, &info);
    set_sensitive(&viewer.metadata, enabled);
}

/// The effective `DocumentInfo` right now — see the module doc for why this
/// is not simply a field read off `Document`. Takes the two session fields
/// it actually needs rather than a whole `&DocumentSession`, so a unit test
/// can call it without constructing the rest of that (much larger) struct.
fn current_document_info(
    document_model: Option<&Document>,
    save_backing: Option<&SaveBacking>,
) -> DocumentInfo {
    let from_log = document_model.and_then(|document| {
        document
            .pending_edits
            .entries()
            .iter()
            .rev()
            .find_map(|command| match command {
                Command::SetDocumentInfo { after, .. } => Some(after.clone()),
                _ => None,
            })
    });
    from_log.unwrap_or_else(|| {
        save_backing
            .map(|backing| backing.base.document_info())
            .unwrap_or_default()
    })
}

/// Runs one metadata command against the open document, then reports the
/// outcome — the metadata twin of `forms::command::command`. Never redraws
/// the canvas: nothing about `/Info` has an on-page visual representation.
fn command(
    viewer: &Viewer,
    operation: impl FnOnce(&mut DocumentSession) -> Result<String, String>,
) {
    if let Some(refusal) = viewer.content_edit_refusal() {
        viewer.status.set_text(refusal);
        return;
    }
    let result = {
        let mut state = viewer.state.borrow_mut();
        match state.session.as_mut() {
            Some(session) => operation(session),
            None => Err(NO_DOCUMENT.to_string()),
        }
    };
    match result {
        Ok(message) => {
            if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
                session.edit_revision += 1;
                session.unsaved_to_disk = true;
            }
            viewer.status.set_text(&message);
        }
        Err(error) => viewer.status.set_text(&error),
    }
}

/// Unreachable in practice — a session without a model reports
/// `ContentEditAccess::Unavailable`, which [`command`]'s own refusal check
/// already refuses before this ever runs — but it reports the exact same
/// message that refusal does, so the two can never contradict each other.
fn model(session: &mut DocumentSession) -> Result<&mut Document, String> {
    session
        .document_model
        .as_mut()
        .ok_or_else(|| CONTENT_MODEL_UNAVAILABLE.to_string())
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    log.apply(document, command);
    document.pending_edits = log;
}

/// Records one `SetDocumentInfo`: `before` is the effective value read fresh
/// from the model (never a value captured earlier), `after` is `before` with
/// `mutate` applied. A `mutate` that changes nothing records nothing — most
/// often a date field's Enter/focus-out firing with the display text
/// unchanged, which must not spam the undo stack with a no-op step.
fn set_info(viewer: &Viewer, mutate: impl FnOnce(&mut DocumentInfo)) {
    command(viewer, move |session| {
        let before = current_document_info(
            session.document_model.as_ref(),
            session.save_backing.as_ref(),
        );
        let mut after = before.clone();
        mutate(&mut after);
        if after == before {
            return Ok("No change to record.".to_string());
        }
        let document = model(session)?;
        apply_command(document, Command::SetDocumentInfo { before, after });
        Ok("Document properties updated. Changes are pending save.".to_string())
    });
}

/// `""` becomes `None` (batch decision 3: an absent key, not an empty
/// string) — everything else is `Some` verbatim, untrimmed: unlike a form
/// field's `/T` name, free text like `Keywords` has no uniqueness constraint
/// to justify trimming it on the user's behalf.
fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

fn wire_text_entry(
    viewer: &Viewer,
    entry: &Entry,
    apply: impl Fn(&mut DocumentInfo, Option<String>) + Clone + 'static,
) {
    entry.connect_changed({
        let viewer = viewer.clone();
        move |entry| {
            if viewer.metadata.syncing.get() {
                return;
            }
            let text = non_empty(entry.text().as_str());
            let apply = apply.clone();
            set_info(&viewer, move |info| apply(info, text));
        }
    });
}

/// Parses `raw` and, if it names a real change, applies it via [`set_info`];
/// either way (accepted, rejected, or unchanged) forces a [`refresh`] — see
/// the module doc for why a date field, unlike a text field, always needs
/// one.
fn commit_date(viewer: &Viewer, raw: String, which: DateField) {
    let offset = match which {
        DateField::Creation => viewer.metadata.creation_offset.get(),
        DateField::Mod => viewer.metadata.mod_offset.get(),
    };
    match parse_friendly_date(&raw) {
        Ok(components) => {
            let date = components.map(|parsed| PdfDate {
                year: parsed.year,
                month: parsed.month,
                day: parsed.day,
                hour: parsed.hour,
                minute: parsed.minute,
                second: parsed.second,
                offset,
            });
            set_info(viewer, move |info| match which {
                DateField::Creation => info.creation_date = date,
                DateField::Mod => info.mod_date = date,
            });
        }
        Err(error) => viewer
            .status
            .set_text(&format!("Invalid date, reverted: {error}")),
    }
    refresh(viewer);
}

fn wire_date_entry(viewer: &Viewer, entry: &Entry, which: DateField) {
    entry.connect_activate({
        let viewer = viewer.clone();
        let entry = entry.clone();
        move |_| {
            if viewer.metadata.syncing.get() {
                return;
            }
            commit_date(&viewer, entry.text().to_string(), which);
        }
    });
    let leave_controller = gtk::EventControllerFocus::new();
    leave_controller.connect_leave({
        let viewer = viewer.clone();
        let entry = entry.clone();
        move |_| {
            if viewer.metadata.syncing.get() {
                return;
            }
            commit_date(&viewer, entry.text().to_string(), which);
        }
    });
    entry.add_controller(leave_controller);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{Color, FontFamily, PdfDateOffset, TextStyle};
    use pdf_document::{EditLog, FormField, FormFieldId, PageId, Rect};

    fn a_date(year: u16) -> PdfDate {
        PdfDate {
            year,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            offset: PdfDateOffset::Utc,
        }
    }

    fn document_with_log(entries: Vec<Command>) -> Document {
        let mut document = Document::blank();
        let mut log = EditLog::new();
        for command in entries {
            log.apply(&mut document, command);
        }
        document.pending_edits = log;
        document
    }

    fn sample_field() -> FormField {
        pdf_form::text_field(
            FormFieldId(1),
            PageId(0),
            "Text_1",
            Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            TextStyle {
                font: FontFamily::Helvetica,
                size_pt: 12.0,
                color: Color { r: 0, g: 0, b: 0 },
            },
            false,
            None,
        )
    }

    #[test]
    fn non_empty_collapses_a_blank_string_to_none() {
        assert_eq!(non_empty(""), None);
        assert_eq!(non_empty("Vitela"), Some("Vitela".to_string()));
    }

    #[test]
    fn current_info_falls_back_to_default_with_no_pending_command_and_no_save_backing() {
        let document = document_with_log(vec![]);
        let info = current_document_info(Some(&document), None);
        assert_eq!(info, DocumentInfo::default());
    }

    #[test]
    fn current_info_prefers_the_most_recent_pending_set_document_info() {
        let older = Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: DocumentInfo {
                title: Some("First".to_string()),
                ..Default::default()
            },
        };
        let newer = Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: DocumentInfo {
                title: Some("Second".to_string()),
                creation_date: Some(a_date(2026)),
                ..Default::default()
            },
        };
        let document = document_with_log(vec![older, newer]);

        let info = current_document_info(Some(&document), None);

        assert_eq!(info.title.as_deref(), Some("Second"));
        assert_eq!(info.creation_date, Some(a_date(2026)));
    }

    #[test]
    fn current_info_ignores_unrelated_commands_in_the_same_log() {
        let info_edit = Command::SetDocumentInfo {
            before: DocumentInfo::default(),
            after: DocumentInfo {
                author: Some("A. Editor".to_string()),
                ..Default::default()
            },
        };
        let unrelated = Command::AddFormField(sample_field());
        let document = document_with_log(vec![info_edit, unrelated]);

        let info = current_document_info(Some(&document), None);

        assert_eq!(info.author.as_deref(), Some("A. Editor"));
    }

    #[test]
    fn current_info_falls_back_to_default_with_no_document_model_at_all() {
        let info = current_document_info(None, None);
        assert_eq!(info, DocumentInfo::default());
    }

    fn a_friendly_date(
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> FriendlyDate {
        FriendlyDate {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn parse_friendly_date_accepts_the_full_form() {
        assert_eq!(
            parse_friendly_date("2026-09-01 08:30:45"),
            Ok(Some(a_friendly_date(2026, 9, 1, 8, 30, 45)))
        );
    }

    #[test]
    fn parse_friendly_date_defaults_seconds_when_omitted() {
        assert_eq!(
            parse_friendly_date("2026-09-01 08:30"),
            Ok(Some(a_friendly_date(2026, 9, 1, 8, 30, 0)))
        );
    }

    #[test]
    fn parse_friendly_date_defaults_to_midnight_when_time_is_omitted() {
        assert_eq!(
            parse_friendly_date("2026-09-01"),
            Ok(Some(a_friendly_date(2026, 9, 1, 0, 0, 0)))
        );
    }

    #[test]
    fn parse_friendly_date_treats_blank_input_as_clearing_the_field() {
        assert_eq!(parse_friendly_date(""), Ok(None));
        assert_eq!(parse_friendly_date("   "), Ok(None));
    }

    #[test]
    fn parse_friendly_date_rejects_an_out_of_range_month() {
        assert!(parse_friendly_date("2026-13-01").is_err());
    }

    #[test]
    fn parse_friendly_date_rejects_an_out_of_range_hour() {
        assert!(parse_friendly_date("2026-09-01 25:00").is_err());
    }

    #[test]
    fn parse_friendly_date_rejects_garbage() {
        assert!(parse_friendly_date("not a date").is_err());
    }

    #[test]
    fn parse_friendly_date_rejects_a_missing_day() {
        assert!(parse_friendly_date("2026-09").is_err());
    }

    #[test]
    fn friendly_display_is_empty_for_no_date() {
        assert_eq!(friendly_display(None), "");
    }

    #[test]
    fn friendly_display_and_parse_round_trip_the_same_components() {
        let date = PdfDate {
            year: 2026,
            month: 9,
            day: 1,
            hour: 8,
            minute: 30,
            second: 45,
            offset: PdfDateOffset::Plus {
                hours: 5,
                minutes: 30,
            },
        };
        let displayed = friendly_display(Some(date));
        assert_eq!(displayed, "2026-09-01 08:30:45");
        assert_eq!(
            parse_friendly_date(&displayed),
            Ok(Some(a_friendly_date(2026, 9, 1, 8, 30, 45))),
            "offset is intentionally not part of the round trip — see the module doc"
        );
    }
}
