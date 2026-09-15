//! The Organize header row as a *row*: what it demands in width, and what it
//! does when it cannot have it.
//!
//! The controls themselves are tested where they belong — `add_pdfs`,
//! `extract` and `split` each own their button's placement and wiring. What
//! is left, and what this file is for, is the property none of them can see
//! on its own: the row has to survive a window narrower than the sum of its
//! parts.

use super::*;

/// The width the whole row insists on before anything is drawn.
///
/// This is the regression. As a plain horizontal `GtkBox` the header measured
/// **619px** minimum and the Organize page around it 651px, because a
/// `GtkBox` reports the sum of its children. Add the ~172px navigation rail
/// and the window could not be honestly shown below ~823px — and when a
/// compositor forced it narrower anyway, GTK under-allocated and stopped
/// drawing the overflow. Save is last in the row, so Save was the first thing
/// to disappear, with nothing on screen to say so.
///
/// A `FlowBox` measures its widest child instead, so the ceiling collapses
/// to roughly one button. The bound below is deliberately loose: what matters
/// is that it is a small multiple of a button and not a sum of six, so the
/// assertion keeps its meaning if a label is translated or a theme pads
/// differently.
#[gtk::test]
fn gtk_ui_the_header_does_not_demand_the_width_of_all_its_buttons_at_once() {
    with_organize(|viewer| {
        let header = header_of(viewer);
        let (minimum, _, _, _) = header.measure(gtk::Orientation::Horizontal, -1);

        assert!(
            minimum < 320,
            "the header row insists on {minimum}px; it used to be 619 and that \
             is what pushed Save off the screen"
        );
    });
}

/// The other half of the same property, and the one worth keeping: the row
/// of buttons is no longer what makes this screen wide.
///
/// The page still has a floor — measured at 454px, of which 422 is the views
/// `Stack`, because the page grid asks for two cards abreast
/// (`min_children_per_line(2)`). That is *content* deciding how much room it
/// needs to be worth showing, which is a defensible minimum and a separate
/// question from chrome refusing to fold. What must never come back is the
/// header being the binding constraint, so that is what this asserts, rather
/// than a number about a grid this change does not touch.
#[gtk::test]
fn gtk_ui_the_header_is_no_longer_what_makes_the_screen_wide() {
    with_organize(|viewer| {
        let header = header_of(viewer);
        let page = header
            .parent()
            .expect("the header sits in the organize page");
        let (header_minimum, _, _, _) = header.measure(gtk::Orientation::Horizontal, -1);

        let widest_sibling =
            std::iter::successors(page.first_child(), |child| child.next_sibling())
                .filter(|child| child != &header)
                .map(|child| child.measure(gtk::Orientation::Horizontal, -1).0)
                .max()
                .expect("the page holds more than the header");

        assert!(
            header_minimum < widest_sibling,
            "the header insists on {header_minimum}px against {widest_sibling}px for the \
             widest thing below it — the row is setting the screen's floor again"
        );
    });
}

/// Wrapping must not reorder. A `FlowBox` with a sort function would be free
/// to, and the page grid in this same screen has one — this row deliberately
/// does not, so the order is insertion order for ever.
#[gtk::test]
fn gtk_ui_the_actions_keep_their_order_in_one_container() {
    with_organize(|viewer| {
        let actions = action_slot(&viewer.organize.save_button)
            .parent()
            .expect("the actions share a container");

        let labels: Vec<String> =
            std::iter::successors(actions.first_child(), |slot| slot.next_sibling())
                .filter_map(|slot| {
                    slot.first_child()
                        .and_then(|child| child.downcast::<Button>().ok())
                        .and_then(|button| button.label())
                        .map(|label| label.to_string())
                })
                .collect();

        assert_eq!(
            labels,
            ["Undo", "Redo", "Add PDFs", "Extract", "Split", "Save"],
            "left to right, whatever line each ends up on"
        );
    });
}

/// The heading yields its width before the actions wrap — the cheapest
/// concession in the row, and the only one a user cannot lose a command to.
#[gtk::test]
fn gtk_ui_the_heading_ellipsizes_rather_than_holding_width_it_does_not_need() {
    with_organize(|viewer| {
        let heading = header_of(viewer)
            .first_child()
            .expect("the heading is the first thing in the row")
            .downcast::<gtk::Label>()
            .expect("the heading is a label");

        assert_eq!(heading.text().as_str(), "Organize pages");
        assert_eq!(heading.ellipsize(), gtk::pango::EllipsizeMode::End);
    });
}

/// The row itself, reached through a control that is always in it.
fn header_of(viewer: &Viewer) -> gtk::Widget {
    action_slot(&viewer.organize.save_button)
        .parent()
        .expect("the actions share a container")
        .parent()
        .expect("the actions container sits in the header row")
}
