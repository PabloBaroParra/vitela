//! Both views' quarter-turns: what a click records, what it costs the cards
//! on screen, and the one permission they ask that their neighbours in the
//! same row do not.
//!
//! Split out of [`super::history`] and [`super::refusals`] rather than added
//! to either, because a rotation is the odd one out on both counts. Every
//! other operation on this screen changes the page *list* — so it forces
//! `pdf-save`'s full-rewrite writer, and so it invalidates nothing about how
//! a surviving page looks. A turn does neither: it stays on the incremental
//! writer, and it makes exactly the cards it touched pictures of an angle
//! their pages no longer have.
//!
//! The Pages view's per-page turn ([`page`]) and the Documents view's
//! per-block one ([`block`]) are siblings here rather than each sitting beside
//! its own view's other tests, for that same reason: what makes a turn worth
//! testing is identical in both, and the contrast between them — one card
//! against a whole block, one `PageId` against a list of them — is the point.
//! What they share is right here: the angles they are judged by.

use super::*;
use pdf_document::Rotation;

mod block;
mod page;

/// The recorded angle of each model page, in `Document.pages` order.
fn rotations(viewer: &Viewer) -> Vec<Rotation> {
    session(viewer)
        .document_model
        .as_ref()
        .unwrap()
        .pages
        .iter()
        .map(|page| page.rotation)
        .collect()
}
