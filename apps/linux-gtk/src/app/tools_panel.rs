//! The right-hand panel's own chrome: the Tools/Comments/Fill & Sign tab
//! switcher and the document-properties readout, mirroring `shell`'s left
//! rail on the other side of the canvas.
//!
//! The tools tab wraps the existing annotation/content-edit controls
//! unchanged — this module owns navigation and layout around them, not their
//! behavior, which stays with `annotations`/`content_edit`.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{
    Box as GtkBox, FlowBox, Label, Orientation, ScrolledWindow, SelectionMode, Stack, ToggleButton,
};

/// The panel's pages, in strip order: the `Stack` child name and the label its
/// tab carries. One list rather than two parallel ones, so a page can never be
/// added to the stack without a tab to reach it by.
const TABS: [(&str, &str); 3] = [
    ("tools", "Tools"),
    ("comments", "Comments"),
    (FILL_SIGN_PAGE, "Fill & Sign"),
];

/// The `Stack` child name for the "Fill & Sign" page — shared with `build_ui`
/// so the app rail's "Sign" button (T-186) can switch to it by name instead
/// of duplicating the literal.
pub(crate) const FILL_SIGN_PAGE: &str = "fill-sign";

/// Shown for a property this document simply does not have, and for every
/// field before any document is open. Distinguishes "read and empty" from a
/// blank label that might just not have painted yet. Used only by
/// `metadata`'s read-only "Pages" row now — its editable `/Info` fields show
/// an empty `Entry` instead, since a blank text box already reads as "no
/// value" without needing a placeholder glyph.
pub(crate) const EMPTY: &str = "\u{2013}";

/// Builds the right panel's content: a Tools/Comments/Fill & Sign switcher
/// over a `Stack`, with `annotation_row`/`content_edit_row` embedded
/// unchanged under Tools, followed by the document-properties panel
/// (`metadata_content`, built and owned by the `metadata` module — T-176).
/// Comments has no feature behind it yet, so its page says so rather than
/// showing empty space that looks broken. Fill & Sign stacks `forms_content`
/// (T-141/T-142's field placement and fill controls) over `sign_content`
/// (Batch B23 Fase 2's certificate chooser) — two features sharing one tab,
/// the way Adobe's own "Fill & Sign" does.
///
/// Returns the `Stack` alongside the panel so `build_ui` can drive it from
/// outside — T-186 needs it to switch to `FILL_SIGN_PAGE` when the app
/// rail's "Sign" button is clicked, the same way the tab switcher itself
/// does internally.
pub(crate) fn build_tools_panel(
    annotation_row: &ScrolledWindow,
    content_edit_row: &FlowBox,
    forms_content: &GtkBox,
    sign_content: &GtkBox,
    metadata_content: &GtkBox,
) -> (GtkBox, Stack) {
    let tools_page = GtkBox::new(Orientation::Vertical, 10);
    tools_page.append(&panel_heading("Annotations"));
    tools_page.append(annotation_row);
    tools_page.append(&panel_heading("Content"));
    tools_page.append(content_edit_row);
    tools_page.append(metadata_content);

    let stack = Stack::new();
    stack.set_vexpand(true);
    // Same reasoning as `side_panel::collapsible`'s own `hhomogeneous(false)`/
    // `vhomogeneous(false)`: a `Stack` otherwise allocates every page the
    // size of its tallest/widest one, so Tools' page (annotations +
    // content-edit + the full metadata panel) would force Comments and
    // Fill & Sign to carry its height — and, now that Fill & Sign stacks
    // `forms_content` under `sign_content`, the reverse direction is live
    // too: whichever page is tallest would pad every other page's blank
    // space instead of each page sizing to its own content.
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);
    stack.add_named(&tools_page, Some("tools"));
    stack.add_named(
        &placeholder_page("Comments aren't available in this shell yet."),
        Some("comments"),
    );
    // T-141 fills `forms_content` with the field-placement toolbar and style
    // inspector; T-142's fill panel joins it later. `sign_content` (Fase 2)
    // adds the certificate chooser below it. No placeholder fallback: both
    // are real here, not deferred like Comments.
    let fill_sign_page = GtkBox::new(Orientation::Vertical, 10);
    fill_sign_page.append(forms_content);
    fill_sign_page.append(sign_content);
    stack.add_named(&fill_sign_page, Some(FILL_SIGN_PAGE));

    let switcher = build_tab_switcher(&stack);

    let panel = GtkBox::new(Orientation::Vertical, 10);
    // Pinned explicitly rather than left to compute from children: this
    // panel's own width is the resizable pane's job (see `build_ui`'s
    // `Paned`), not something a label three levels down should be able to
    // override by requesting extra space.
    panel.set_hexpand(false);
    panel.append(&switcher);
    panel.append(&stack);

    (panel, stack)
}

/// The panel's tab strip, driving `stack`.
///
/// A `FlowBox` of `ToggleButton`s and **not** the `GtkStackSwitcher` this
/// replaced, which is the entire point of the function existing. A
/// `StackSwitcher` is a rigid horizontal row: measured here it reported a
/// minimum of 400px *and* a natural of 400px — it does not shrink by a single
/// pixel. That made it, not the controls under it, the floor on how narrow
/// the tools panel could be drawn (the `Stack` beneath it measures 142px),
/// and dragging the canvas/tools divider past that floor did not narrow the
/// panel, it cut the strip off at the window edge — the user-visible symptom
/// of "all the buttons disappear".
///
/// Same lesson as `editor_toolbar` and `annotations::toolbar`: any horizontal
/// row of labelled controls in this shell has to be able to wrap. Do not swap
/// a `StackSwitcher` back in.
///
/// The `Stack` stays the single source of truth for which page is up — the
/// toggles report what it settled on rather than tracking their own state, so
/// the strip cannot come to disagree with what is on screen, and clicking the
/// already-active tab re-asserts it instead of leaving every toggle off.
fn build_tab_switcher(stack: &Stack) -> FlowBox {
    let switcher = FlowBox::new();
    switcher.add_css_class("tools-tab-switcher");
    switcher.set_selection_mode(SelectionMode::None);
    switcher.set_homogeneous(false);
    switcher.set_row_spacing(4);
    switcher.set_column_spacing(4);
    switcher.set_max_children_per_line(TABS.len() as u32);

    let toggles: Rc<Vec<(&'static str, ToggleButton)>> = Rc::new(
        TABS.iter()
            .map(|(name, title)| {
                let toggle = ToggleButton::with_label(title);
                switcher.append(&toggle);
                (*name, toggle)
            })
            .collect(),
    );

    // `set_active` below re-enters nothing today, but the guard states the
    // invariant rather than relying on which of GTK's toggle signals happens
    // to fire: syncing the strip must never be able to drive a page change.
    let syncing = Rc::new(Cell::new(false));
    let sync: Rc<dyn Fn()> = {
        let stack = stack.clone();
        let toggles = toggles.clone();
        let syncing = syncing.clone();
        Rc::new(move || {
            syncing.set(true);
            let current = stack.visible_child_name();
            for (name, toggle) in toggles.iter() {
                toggle.set_active(current.as_deref() == Some(*name));
            }
            syncing.set(false);
        })
    };

    for (name, toggle) in toggles.iter() {
        toggle.connect_clicked({
            let stack = stack.clone();
            let sync = sync.clone();
            let syncing = syncing.clone();
            let name = *name;
            move |_| {
                if syncing.get() {
                    return;
                }
                stack.set_visible_child_name(name);
                sync();
            }
        });
    }
    stack.connect_visible_child_notify({
        let sync = sync.clone();
        move |_| sync()
    });
    sync();

    switcher
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

    /// T-186: the app rail's "Sign" button (`shell::build_app_rail`) drives
    /// this exact `Stack` by name from outside `tools_panel` — this pins the
    /// two facts that call site depends on: `build_tools_panel` hands the
    /// `Stack` back, and `FILL_SIGN_PAGE` is a real page on it (built from
    /// the same `forms_content`/`sign_content` widgets `build_ui` passes in,
    /// not a placeholder), reachable by `set_visible_child_name`.
    #[gtk::test]
    fn gtk_ui_the_stack_switches_to_the_fill_sign_page_by_its_shared_constant() {
        let annotation_row = ScrolledWindow::new();
        let content_edit_row = FlowBox::new();
        let (_forms_toolbar, forms_content) = crate::app::forms::build_forms_content();
        let (_choose_pfx, _choose_pkcs11, _choose_nss, _signed_indicator, sign_content) =
            crate::app::sign::build_sign_content();
        let (_metadata_panel, metadata_content) = crate::app::metadata::build_metadata_panel();

        let (_panel, stack) = build_tools_panel(
            &annotation_row,
            &content_edit_row,
            &forms_content,
            &sign_content,
            &metadata_content,
        );

        assert_eq!(stack.visible_child_name().as_deref(), Some("tools"));
        stack.set_visible_child_name(FILL_SIGN_PAGE);
        assert_eq!(stack.visible_child_name().as_deref(), Some(FILL_SIGN_PAGE));
    }

    fn stack_of_tabs() -> Stack {
        let stack = Stack::new();
        for (name, _) in TABS {
            stack.add_named(&Label::new(Some(name)), Some(name));
        }
        stack
    }

    /// The strip's toggles, in order. `FlowBox` wraps each child in a
    /// `FlowBoxChild` of its own, so the buttons sit one level down.
    fn tab_toggles(switcher: &FlowBox) -> Vec<ToggleButton> {
        std::iter::successors(switcher.first_child(), |child| child.next_sibling())
            .filter_map(|child| child.first_child())
            .filter_map(|child| child.downcast::<ToggleButton>().ok())
            .collect()
    }

    /// The regression [`build_tab_switcher`] exists for. The `StackSwitcher`
    /// it replaced measured 400px minimum *and* 400px natural — a floor the
    /// controls underneath it (142px) never asked for, and the reason
    /// dragging the canvas/tools divider right cut the whole panel off at the
    /// window edge instead of narrowing it.
    ///
    /// Measured against the sum of the tabs rather than against the
    /// `FlowBox`'s own natural width: the sum is what a rigid strip demands,
    /// and `FlowBox`'s reported natural width is environment-dependent (it
    /// came back equal to the minimum under CI's Xvfb), which is not the
    /// property under test.
    #[gtk::test]
    fn gtk_ui_tab_strip_shrinks_instead_of_setting_the_panels_floor() {
        let switcher = build_tab_switcher(&stack_of_tabs());

        let (minimum, _, _, _) = switcher.measure(Orientation::Horizontal, -1);
        let sum_of_tabs: i32 =
            std::iter::successors(switcher.first_child(), |child| child.next_sibling())
                .map(|tab| tab.measure(Orientation::Horizontal, -1).1)
                .sum();

        assert!(
            minimum * 2 <= sum_of_tabs,
            "tab strip minimum {minimum} is not meaningfully below the {sum_of_tabs} its tabs add \
             up to; it is behaving like the StackSwitcher it replaced and will floor the panel"
        );
    }

    #[gtk::test]
    fn gtk_ui_clicking_a_tab_moves_the_stack_and_presses_only_that_tab() {
        let stack = stack_of_tabs();
        let toggles = tab_toggles(&build_tab_switcher(&stack));

        toggles[2].emit_clicked();

        assert_eq!(stack.visible_child_name().as_deref(), Some(TABS[2].0));
        assert!(toggles[2].is_active());
        assert_eq!(toggles.iter().filter(|tab| tab.is_active()).count(), 1);
    }

    /// Clicking the tab that is already open re-asserts it. This is the whole
    /// reason the `Stack` owns the state and the toggles only report it: a
    /// `ToggleButton` left to track its own would untoggle here, leaving the
    /// strip claiming no page is open while one plainly is.
    #[gtk::test]
    fn gtk_ui_clicking_the_open_tab_leaves_it_open_not_unpressed() {
        let stack = stack_of_tabs();
        let toggles = tab_toggles(&build_tab_switcher(&stack));

        toggles[0].emit_clicked();

        assert_eq!(stack.visible_child_name().as_deref(), Some(TABS[0].0));
        assert!(toggles[0].is_active());
    }

    /// The other direction: whatever moves the stack, the strip follows.
    #[gtk::test]
    fn gtk_ui_moving_the_stack_directly_repoints_the_strip() {
        let stack = stack_of_tabs();
        let toggles = tab_toggles(&build_tab_switcher(&stack));

        stack.set_visible_child_name(TABS[1].0);

        assert!(toggles[1].is_active());
        assert_eq!(toggles.iter().filter(|tab| tab.is_active()).count(), 1);
    }
}
