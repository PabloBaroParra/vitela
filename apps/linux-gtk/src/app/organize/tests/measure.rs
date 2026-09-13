//! What the Organize screen costs in *time* on a large document (checklist
//! §11, item 12) — the half of that item a render count cannot answer.
//!
//! [`super::thumbnails`] proves a view switch asks pdfium for nothing once
//! the cache is warm. What is left after that is widget work: building a card
//! per page, and letting the `Stack` swap two views that already hold theirs.
//! This module puts a stopwatch on exactly that, with the renderer stubbed
//! out by [`super::capture_thumbnail`] — which is faithful to the screen,
//! since a real thumbnail render happens off the main thread and the numbers
//! for it are measured where they belong, in
//! `pdf-save/tests/perf_large_assembly.rs`.
//!
//! `#[ignore]`d: four hundred cards is far past what any behavior test needs,
//! and a wall-clock number is not an assertion — it is a measurement, and one
//! that depends on the machine it ran on. Run it explicitly:
//!
//! ```sh
//! cargo test -p linux-gtk --bin linux-gtk -- --ignored --nocapture gtk_ui_measure
//! ```

use std::time::Instant;

use super::*;

/// A four-hundred-page document: the assembly `perf_large_assembly` measures,
/// so both halves of the item describe the same screen.
const PAGES: u32 = 400;

#[gtk::test]
#[ignore = "measurement, not an assertion: 400 cards and wall-clock numbers; run explicitly, see module docs"]
fn gtk_ui_measure_a_large_documents_grid_and_view_switch() {
    with_organize_of(PAGES, |viewer| {
        let start = Instant::now();
        populate_grid(viewer);
        let populate = start.elapsed();
        assert_eq!(viewer.organize.cards.len(), PAGES as usize);

        let start = Instant::now();
        viewer.organize.documents_toggle.set_active(true);
        let to_documents = start.elapsed();

        let start = Instant::now();
        viewer.organize.pages_toggle.set_active(true);
        let to_pages = start.elapsed();

        println!("\n{PAGES} pages, thumbnail cache warm\n");
        for (label, elapsed) in [
            ("populate the grid", populate),
            ("switch to Documents", to_documents),
            ("switch back to Pages", to_pages),
        ] {
            println!("  {label:<22} {:>9.1} ms", elapsed.as_secs_f64() * 1000.0);
        }
    });
}
