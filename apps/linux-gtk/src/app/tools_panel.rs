//! The right-hand panel's own pages: Edit, Annotate and Fill & Sign over a
//! `Stack`, plus the document-properties readout under Annotate.
//!
//! Each page wraps content built by the feature module that owns it —
//! `annotations` and `metadata` under Annotate, `content_edit::panel` under
//! Edit, `forms` and `sign` under Fill & Sign. This module owns the layout
//! around them, never their behavior.
//!
//! **The panel has no tab strip of its own.** The top mode bar (`mode_bar`)
//! picks the page: its Edit, Annotate and Sign modes land on the page of the
//! same name, and it follows this `Stack` back, so a page changed from
//! anywhere else still shows as the current mode.
//!
//! Comments, which had a tab of its own while it was a placeholder, is a
//! section at the foot of the Annotate page instead: the mode bar has no
//! Comments mode, and a comment is an annotation.

use gtk::prelude::*;
use gtk::{Box as GtkBox, Label, Orientation, ScrolledWindow, Stack};

use super::i18n::tr;

/// The `Stack` child name for the "Fill & Sign" page — shared so the mode
/// bar's Sign mode (T-186) can switch to it by name, and read it back,
/// instead of duplicating the literal.
pub(crate) const FILL_SIGN_PAGE: &str = "fill-sign";

/// The `Stack` child name for the "Edit" page, shared for the same reason
/// [`FILL_SIGN_PAGE`] is: the mode bar's Edit mode and Home's "Edit" tile
/// both navigate to it through `home::tools::apply`.
pub(crate) const EDIT_PAGE: &str = "edit";

/// The `Stack` child name for the "Annotate" page — the annotation toolbar,
/// the document properties and the Comments section. Shared for the same
/// reason as its two neighbours: the mode bar's Annotate mode reveals a
/// control on this page, and focusing a widget on a page the `Stack` is not
/// showing reveals nothing at all.
pub(crate) const ANNOTATE_PAGE: &str = "annotate";

/// Shown for a property this document simply does not have, and for every
/// field before any document is open. Distinguishes "read and empty" from a
/// blank label that might just not have painted yet. Used only by
/// `metadata`'s read-only "Pages" row now — its editable `/Info` fields show
/// an empty `Entry` instead, since a blank text box already reads as "no
/// value" without needing a placeholder glyph.
pub(crate) const EMPTY: &str = "\u{2013}";

/// Builds the right panel's content: a `Stack` of the Annotate, Edit and
/// Fill & Sign pages. Annotate holds `annotation_row` unchanged, then the
/// document-properties panel (`metadata_content`, built and owned by the
/// `metadata` module — T-176), then a Comments section that says comments are
/// not available yet rather than leaving a gap that looks broken. Fill & Sign
/// stacks `forms_content` (T-141/T-142's field placement and fill controls)
/// over `sign_content` (Batch B23 Fase 2's certificate chooser) — two
/// features sharing one page, the way Adobe's own "Fill & Sign" does.
///
/// `edit_content` (built and owned by `content_edit::panel`) is a page of its
/// own rather than the "Content" section it used to be on the page next door.
/// See that module's own docs for the defects the move fixed.
///
/// Returns the `Stack` alongside the panel so `build_ui` can hand it to the
/// mode bar, which switches it by page name and follows it back.
pub(crate) fn build_tools_panel(
    annotation_row: &ScrolledWindow,
    edit_content: &GtkBox,
    forms_content: &GtkBox,
    sign_content: &GtkBox,
    metadata_content: &GtkBox,
) -> (GtkBox, Stack) {
    let annotate_page = GtkBox::new(Orientation::Vertical, 10);
    annotate_page.append(&panel_heading(&tr("tools-annotations")));
    annotate_page.append(annotation_row);
    annotate_page.append(metadata_content);
    annotate_page.append(&panel_heading(&tr("tools-comments")));
    annotate_page.append(&placeholder_page(&tr("tools-comments-unavailable")));

    let stack = Stack::new();
    stack.set_vexpand(true);
    // Same reasoning as `side_panel::collapsible`'s own `hhomogeneous(false)`/
    // `vhomogeneous(false)`: a `Stack` otherwise allocates every page the
    // size of its tallest/widest one, so the Annotate page (the annotation
    // toolbar plus the full metadata panel) would force Edit and Fill & Sign
    // to carry its height — and the reverse direction is live too: whichever
    // page is tallest would pad every other page's blank space instead of
    // each page sizing to its own content.
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);
    // Annotate first: a `Stack` shows its first child until told otherwise,
    // and Annotate is the editor's landing page.
    stack.add_named(&annotate_page, Some(ANNOTATE_PAGE));
    stack.add_named(edit_content, Some(EDIT_PAGE));
    // T-141 fills `forms_content` with the field-placement toolbar and style
    // inspector; T-142's fill panel joins it later. `sign_content` (Fase 2)
    // adds the certificate chooser below it.
    let fill_sign_page = GtkBox::new(Orientation::Vertical, 10);
    fill_sign_page.append(forms_content);
    fill_sign_page.append(sign_content);
    stack.add_named(&fill_sign_page, Some(FILL_SIGN_PAGE));

    let panel = GtkBox::new(Orientation::Vertical, 10);
    // Pinned explicitly rather than left to compute from children: this
    // panel's own width is the resizable pane's job (see `build_ui`'s
    // `Paned`), not something a label three levels down should be able to
    // override by requesting extra space.
    panel.set_hexpand(false);
    panel.append(&stack);

    (panel, stack)
}

/// Appends one "key: value" row to `container` and returns the value label
/// for later updates. `pub(crate)`: `metadata` reuses this for its read-only
/// "Pages" row, rather than duplicating the CSS-class wiring a second time —
/// same reasoning as [`panel_heading`] being shared.
pub(crate) fn property_row(container: &GtkBox, key: &str) -> Label {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.add_css_class("property-row");

    let key_label = Label::new(Some(key));
    key_label.set_xalign(0.0);
    key_label.set_width_chars(8);
    key_label.add_css_class("property-key");

    let value_label = Label::new(Some(EMPTY));
    value_label.set_xalign(0.0);
    value_label.set_wrap(true);
    // Caps the label's own natural (unwrapped) width request rather than
    // relying on `hexpand` to keep it in bounds — `hexpand` on a widget this
    // deep propagates up through every ancestor `GtkBox` that hasn't pinned
    // its own `hexpand` (GTK's `compute_expand`), which previously made the
    // whole right panel greedily hexpand and starve the page canvas next to
    // it. A long `/Title` now wraps inside the row instead of stretching it.
    value_label.set_max_width_chars(20);
    value_label.add_css_class("property-value");

    row.append(&key_label);
    row.append(&value_label);
    container.append(&row);
    value_label
}

/// `pub(crate)`: `forms::toolbar` reuses this for its own "Form fields"/
/// "Field style" section headings, rather than duplicating the CSS-class
/// wiring a second time.
pub(crate) fn panel_heading(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("panel-heading");
    label
}

fn placeholder_page(message: &str) -> Label {
    let label = Label::new(Some(message));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_valign(gtk::Align::Start);
    label.add_css_class("tools-placeholder");
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-186: the mode bar's Sign mode drives this exact `Stack` by name from
    /// outside `tools_panel` — this pins the two facts that call site depends
    /// on: `build_tools_panel` hands the `Stack` back, and `FILL_SIGN_PAGE`
    /// is a real page on it, reachable by `set_visible_child_name`.
    #[gtk::test]
    fn gtk_ui_the_stack_switches_to_the_fill_sign_page_by_its_shared_constant() {
        let (stack, _edit_content) = built_panel();

        assert_eq!(stack.visible_child_name().as_deref(), Some(ANNOTATE_PAGE));
        stack.set_visible_child_name(FILL_SIGN_PAGE);
        assert_eq!(stack.visible_child_name().as_deref(), Some(FILL_SIGN_PAGE));
    }

    /// The twin of the test above for the "Edit" page: the mode bar's Edit
    /// mode drives this `Stack` by name from `home::tools::apply`, so the
    /// page has to be a real one built from the content `build_ui` passes in.
    #[gtk::test]
    fn gtk_ui_the_stack_switches_to_the_edit_page_by_its_shared_constant() {
        let (stack, edit_content) = built_panel();

        stack.set_visible_child_name(EDIT_PAGE);

        assert_eq!(stack.visible_child_name().as_deref(), Some(EDIT_PAGE));
        assert_eq!(
            stack.visible_child().as_ref(),
            Some(edit_content.upcast_ref::<gtk::Widget>())
        );
    }

    /// Comments has no mode of its own: it is a section of the Annotate page,
    /// still saying it is not available rather than gone missing.
    #[gtk::test]
    fn gtk_ui_comments_is_a_section_of_the_annotate_page() {
        let _spanish = crate::app::i18n::speak(crate::app::i18n::Language::Spanish);
        let (stack, _) = built_panel();

        assert!(stack.child_by_name("comments").is_none());
        let annotate = stack.child_by_name(ANNOTATE_PAGE).unwrap();
        let texts: Vec<String> =
            std::iter::successors(annotate.first_child(), |child| child.next_sibling())
                .filter_map(|child| child.downcast::<Label>().ok())
                .map(|label| label.text().to_string())
                .collect();
        assert_eq!(
            texts,
            [
                "Anotaciones",
                "Comentarios",
                "Los comentarios aún no están disponibles en esta versión.",
            ]
        );
    }

    /// The panel opens on Annotate, the editor's landing page, and the panel
    /// is the `Stack` alone — the tab strip that used to lead it is gone.
    #[gtk::test]
    fn gtk_ui_the_panel_is_the_stack_and_opens_on_annotate() {
        let annotation_row = ScrolledWindow::new();
        let (_edit_controls, edit_content) = crate::app::content_edit::panel::build_edit_content();
        let (_forms_toolbar, forms_content) = crate::app::forms::build_forms_content();
        let (_choose_pfx, _choose_pkcs11, _choose_nss, _signed_indicator, sign_content) =
            crate::app::sign::build_sign_content();
        let (_metadata_panel, metadata_content) = crate::app::metadata::build_metadata_panel();
        let (panel, stack) = build_tools_panel(
            &annotation_row,
            &edit_content,
            &forms_content,
            &sign_content,
            &metadata_content,
        );

        assert_eq!(
            panel.first_child().as_ref(),
            Some(stack.upcast_ref::<gtk::Widget>())
        );
        assert!(stack.next_sibling().is_none());
        assert_eq!(stack.visible_child_name().as_deref(), Some(ANNOTATE_PAGE));
    }

    /// A panel built the way `build_ui` builds it, returning the `Stack` and
    /// the Edit page's own root for identity assertions.
    fn built_panel() -> (Stack, GtkBox) {
        let annotation_row = ScrolledWindow::new();
        let (_edit_controls, edit_content) = crate::app::content_edit::panel::build_edit_content();
        let (_forms_toolbar, forms_content) = crate::app::forms::build_forms_content();
        let (_choose_pfx, _choose_pkcs11, _choose_nss, _signed_indicator, sign_content) =
            crate::app::sign::build_sign_content();
        let (_metadata_panel, metadata_content) = crate::app::metadata::build_metadata_panel();

        let (_panel, stack) = build_tools_panel(
            &annotation_row,
            &edit_content,
            &forms_content,
            &sign_content,
            &metadata_content,
        );

        (stack, edit_content)
    }
}
