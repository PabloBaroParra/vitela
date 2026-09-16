//! One parse, one serialise, and the documents that get neither.
//!
//! ## The rule this module exists to keep
//!
//! *A compression opens the document once, hands the same graph to every
//! stage, and writes it out once.*
//!
//! [`crate::pipeline`] decides the order stages run in; this module is what
//! they run *inside*. The alternative — every stage taking bytes and
//! returning bytes — costs a full parse and a full serialise per stage (on
//! `tests/fixtures/large/perf_200pg.pdf` that is 54 MB each way), and worse,
//! it makes every stage own a copy of [`repacked`], the write format. One
//! envelope, one copy.
//!
//! Split in two by what a question is *about*. [`gate`] answers the one that
//! is about the document — may this file be rewritten at all — and answers it
//! at the door, before a `Session` exists. This file is everything that
//! happens once the answer was yes: the graph, the running tally, the write
//! format, and the one thing checked on the way out.
//!
//! ## Why the candidate is re-read before it is offered
//!
//! [`crate::guarantee`] compares sizes, and the smallest possible PDF is a
//! broken one. A repack that loses pages would win that comparison. So
//! [`Session::finish`] reloads what it just wrote and counts the pages before
//! offering it; if the count moved, the candidate is dropped and the input is
//! offered unchanged. One extra parse per compression, in exchange for the
//! class of bug where a user's document comes back lighter because part of it
//! is gone.
//!
//! That check earns its keep twice over now that [`crate::prune`] deletes
//! objects: a sweep that took one thing too many shows up here as a page that
//! stopped resolving.

mod gate;

pub(crate) use gate::{open, Opened};

use lopdf::{Document, SaveOptions};

use crate::error::CompressError;
use crate::guarantee::Candidate;
use crate::report::Work;

/// A document open for compression.
///
/// Holds the graph every stage mutates, the page count it arrived with, and
/// the running tally of what the stages have done.
pub(crate) struct Session {
    document: Document,
    pages_on_arrival: usize,
    work: Work,
}

impl Session {
    /// A document that [`gate`] has cleared, with nothing done to it yet.
    /// Records the page count it arrived with, which is the only thing
    /// [`Session::finish`] checks on the way out.
    pub(crate) fn new(document: Document) -> Self {
        Session {
            pages_on_arrival: document.get_pages().len(),
            document,
            work: Work::default(),
        }
    }

    /// The graph, for a stage to change.
    pub(crate) fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    /// Adds what a stage just did to the running tally.
    pub(crate) fn record(&mut self, work: Work) {
        self.work = self.work.plus(work);
    }

    /// Writes the document out and offers it — unless it came back with a
    /// different number of pages than it arrived with, in which case `input`
    /// is offered unchanged.
    ///
    /// # Errors
    ///
    /// [`CompressError::Io`] when the document cannot be serialised.
    pub(crate) fn finish(mut self, input: &[u8]) -> Result<Candidate, CompressError> {
        let mut bytes = Vec::with_capacity(input.len());
        self.document.save_with_options(&mut bytes, repacked())?;

        if page_count(&bytes) != Some(self.pages_on_arrival) {
            return Ok(Candidate::new(input.to_vec()));
        }

        Ok(Candidate::new(bytes).with_work(self.work))
    }
}

/// The write format this crate exists to ask for.
///
/// Built as a struct literal rather than through `SaveOptions::builder()` on
/// purpose: the builder's `compression_level` defaults to `0` and `build()`
/// passes it straight through, so a builder nobody remembered to call
/// `.compression_level()` on packs the object streams *uncompressed* — the
/// opposite of the point. `ObjectStreamConfig::default()` is level 6.
///
/// `linearize` stays off. Fast-web-view interleaving is a layout for
/// streaming a file over a network, not a size reduction; it adds a hint
/// table and can make the file bigger.
fn repacked() -> SaveOptions {
    SaveOptions {
        use_object_streams: true,
        use_xref_streams: true,
        linearize: false,
        ..SaveOptions::default()
    }
}

/// How many pages `bytes` has, or `None` if it is not a readable document.
///
/// The only question asked of the candidate before it is offered. Deliberately
/// not "does it render identically" — that needs a rasteriser this crate does
/// not depend on outside its tests, and it lives in
/// `tests/render_unchanged.rs`. This one catches the failure that the
/// never-grow guarantee would otherwise reward.
fn page_count(bytes: &[u8]) -> Option<usize> {
    Document::load_mem(bytes)
        .ok()
        .map(|document| document.get_pages().len())
}

#[cfg(test)]
mod tests {
    use lopdf::Object;

    use super::*;
    use crate::test_fixtures::loose_document;

    fn ready(input: &[u8]) -> Box<Session> {
        match open(input, crate::consent::SignedDocuments::LeaveAlone).expect("readable") {
            Opened::Ready(session) => session,
            Opened::Refused(refusal) => panic!("unexpectedly refused: {refusal:?}"),
        }
    }

    #[test]
    fn a_session_remembers_the_pages_it_opened_with() {
        assert_eq!(ready(&loose_document(7)).pages_on_arrival, 7);
    }

    #[test]
    fn an_untouched_session_reports_no_work() {
        let input = loose_document(3);

        let candidate = ready(&input).finish(&input).expect("serialisable");

        assert_eq!(candidate.work(), Work::default());
    }

    #[test]
    fn recorded_work_accumulates_across_stages() {
        let input = loose_document(2);
        let mut session = ready(&input);
        session.record(Work {
            streams_recompressed: 2,
            ..Work::default()
        });
        session.record(Work {
            objects_dropped: 5,
            ..Work::default()
        });

        let candidate = session.finish(&input).expect("serialisable");

        assert_eq!(
            candidate.work(),
            Work {
                streams_recompressed: 2,
                objects_dropped: 5,
                ..Work::default()
            }
        );
    }

    /// The guard behind the re-read, exercised through a session that lost a
    /// page rather than through the helper: a candidate with the wrong page
    /// count is dropped and the input comes back untouched.
    #[test]
    fn a_session_that_loses_a_page_offers_the_input_instead() {
        let input = loose_document(4);
        let mut session = ready(&input);

        let pages_id = session
            .document_mut()
            .catalog()
            .ok()
            .and_then(|catalog| catalog.get(b"Pages").ok())
            .and_then(|pages| pages.as_reference().ok())
            .expect("the fixture's catalog points at a page tree");

        // Take one kid away behind the session's back.
        if let Ok(Object::Array(kids)) = session
            .document_mut()
            .get_object_mut(pages_id)
            .and_then(|object| object.as_dict_mut())
            .and_then(|dict| dict.get_mut(b"Kids"))
        {
            kids.pop();
        }

        let candidate = session.finish(&input).expect("serialisable");

        assert_eq!(candidate.bytes(), input.as_slice());
        assert_eq!(candidate.work(), Work::default());
    }

    /// `SaveOptions::builder()` leaves `compression_level` at zero, which
    /// writes *uncompressed* object streams. This pins that nothing here uses
    /// it.
    #[test]
    fn the_write_options_ask_for_real_compression() {
        let options = repacked();

        assert!(options.use_object_streams);
        assert!(options.use_xref_streams);
        assert!(!options.linearize);
        assert!(
            options.object_stream_config.compression_level > 0,
            "object streams packed at level 0 are stored, not compressed"
        );
    }

    #[test]
    fn only_a_readable_document_has_a_page_count() {
        assert_eq!(page_count(&loose_document(3)), Some(3));
        assert_eq!(page_count(b"not a document at all"), None);
        assert_eq!(page_count(b"%PDF-1.7\n%%EOF\n"), None);
    }
}
