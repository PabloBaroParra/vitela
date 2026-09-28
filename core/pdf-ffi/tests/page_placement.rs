//! A page carrying `/Rotate` is drawn turned, but everything on it — text
//! runs, search hits, annotations — stays in the page's unrotated space. A
//! shell that cannot link `pdf-render` (Windows) needs both halves of that
//! across the boundary: which way each page is turned, and the transform
//! that reconciles the two spaces. Without them every overlay on a rotated
//! page lands where the page would be if it had never been turned.

use pdf_ffi::{
    apply_edit, create_document_with_blank_page, open_from_bytes, place_point, place_rect,
    point_to_pdf, save_to_bytes, FfiEditCommand, FfiOrientation, FfiPagePlacement, FfiPageRotation,
    FfiPageSize, FfiPoint, FfiRect, FfiSaveIntent, FfiSignatureAcknowledgement,
};

const TURNS: [FfiPageRotation; 4] = [
    FfiPageRotation::None,
    FfiPageRotation::Clockwise90,
    FfiPageRotation::Clockwise180,
    FfiPageRotation::Clockwise270,
];

/// An A4 portrait page as drawn under `rotation`: a quarter turn swaps the
/// drawn width and height, exactly as `page_dimensions` reports them.
fn a4_turned(rotation: FfiPageRotation, scale: f64) -> FfiPagePlacement {
    let quarter = matches!(
        rotation,
        FfiPageRotation::Clockwise90 | FfiPageRotation::Clockwise270
    );
    let (width_pt, height_pt) = if quarter {
        (842.0, 595.0)
    } else {
        (595.0, 842.0)
    };
    FfiPagePlacement {
        width_pt,
        height_pt,
        rotation,
        scale,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn an_untouched_page_reports_no_rotation() {
    let handle = create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("blank document");

    let page = handle.page_dimensions()[0];

    assert_eq!(page.rotation, FfiPageRotation::None);
}

#[test]
fn a_pending_quarter_turn_is_reported_with_the_swapped_size() {
    let handle = create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("blank document");

    apply_edit(
        &handle,
        FfiEditCommand::RotatePage {
            page: 0,
            delta_degrees: 90,
        },
    )
    .expect("rotate");

    let page = handle.page_dimensions()[0];
    assert_eq!(page.rotation, FfiPageRotation::Clockwise90);
    assert_eq!(
        (page.width_pt.round(), page.height_pt.round()),
        (842.0, 595.0)
    );
}

#[test]
fn a_rotation_saved_into_the_file_is_reported_after_reopening() {
    let handle = create_document_with_blank_page(FfiPageSize::A4, FfiOrientation::Portrait)
        .expect("blank document");
    apply_edit(
        &handle,
        FfiEditCommand::RotatePage {
            page: 0,
            delta_degrees: 270,
        },
    )
    .expect("rotate");
    let bytes = save_to_bytes(
        &handle,
        FfiSaveIntent::Default,
        FfiSignatureAcknowledgement::Unacknowledged,
    )
    .expect("save");

    let reopened = open_from_bytes(bytes, None).expect("reopen");

    let page = reopened.page_dimensions()[0];
    assert_eq!(page.rotation, FfiPageRotation::Clockwise270);
    assert_eq!(
        (page.width_pt.round(), page.height_pt.round()),
        (842.0, 595.0)
    );
}

#[test]
fn an_upright_rect_is_only_flipped_and_scaled() {
    let placed = place_rect(
        FfiRect {
            x: 72.0,
            y: 100.0,
            width: 200.0,
            height: 20.0,
        },
        a4_turned(FfiPageRotation::None, 2.0),
    );

    assert!(close(placed.left, 144.0));
    assert!(close(placed.top, (842.0 - 120.0) * 2.0));
    assert!(close(placed.width, 400.0));
    assert!(close(placed.height, 40.0));
}

#[test]
fn a_quarter_turn_stands_a_line_of_text_on_its_side() {
    // A run near the top-left of the unrotated page ends up near the top
    // *right* once the page is turned clockwise, running downwards.
    let placed = place_rect(
        FfiRect {
            x: 72.0,
            y: 800.0,
            width: 200.0,
            height: 20.0,
        },
        a4_turned(FfiPageRotation::Clockwise90, 1.0),
    );

    assert!(close(placed.left, 800.0));
    assert!(close(placed.top, 72.0));
    assert!(close(placed.width, 20.0));
    assert!(close(placed.height, 200.0));
}

#[test]
fn a_placed_point_comes_back_from_the_pointer_unchanged_under_every_turn() {
    // The forward transform reads the drawn size and the inverse the
    // unrotated one; mixing them up passes at 0° and 180° and fails only on a
    // quarter turn, which is why every turn is checked.
    let original = FfiPoint { x: 100.0, y: 700.0 };
    for rotation in TURNS {
        let page = a4_turned(rotation, 1.5);

        let on_screen = place_point(original, page);
        let back = point_to_pdf(on_screen, page);

        assert!(
            close(back.x, original.x) && close(back.y, original.y),
            "{rotation:?}: {back:?}"
        );
    }
}

#[test]
fn a_placed_rect_contains_its_own_corners_under_every_turn() {
    let rect = FfiRect {
        x: 72.0,
        y: 500.0,
        width: 150.0,
        height: 30.0,
    };
    for rotation in TURNS {
        let page = a4_turned(rotation, 1.0);
        let placed = place_rect(rect, page);

        for corner in [
            FfiPoint {
                x: rect.x,
                y: rect.y,
            },
            FfiPoint {
                x: rect.x + rect.width,
                y: rect.y + rect.height,
            },
        ] {
            let point = place_point(corner, page);
            let inside = point.x >= placed.left - 0.01
                && point.x <= placed.left + placed.width + 0.01
                && point.y >= placed.top - 0.01
                && point.y <= placed.top + placed.height + 0.01;
            assert!(inside, "{rotation:?}: {point:?} outside {placed:?}");
        }
    }
}
