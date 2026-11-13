//! Bold / italic / underline toggles on existing runs, through the whole
//! pipeline: recorded through `pdf_save::record` (and its checkpoint), saved,
//! reopened, and read back.

use lopdf::{dictionary, Object, Stream};
use pdf_document::{Command, Document, PageId, TextRun, TextRunLook};
use pdf_manip::LopdfDocument;
use pdf_save::{
    record_content_command, save_document, CheckpointSession, Checkpoints, ImportedSources,
    SaveInput, SaveIntent, SignatureAcknowledgement,
};

const HELLO: &[u8] = b"BT /F1 12 Tf 72 700 Td (Hello) Tj ( world) Tj ET";

fn font(base_font: &str) -> Object {
    Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => base_font,
        "Encoding" => "WinAnsiEncoding",
    })
}

/// A one-page PDF painting `content` with `/F1` Helvetica and, when
/// `with_bold`, a `/F2` Helvetica-Bold the page never uses.
fn pdf_bytes(content: &[u8], with_bold: bool) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let stream = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let mut fonts = dictionary! { "F1" => font("Helvetica") };
    if with_bold {
        fonts.set("F2", font("Helvetica-Bold"));
    }
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => stream,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => fonts },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => 1, "Kids" => vec![page.into()],
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

/// An editing session the way a shell keeps one: the model, the base it is
/// recorded against and that base's bytes, and its checkpoints.
struct Session {
    document: Document,
    base: LopdfDocument,
    bytes: Vec<u8>,
    checkpoints: Checkpoints<(LopdfDocument, Vec<u8>)>,
}

impl Session {
    fn open(bytes: Vec<u8>) -> Self {
        let (base, security) = pdf_manip::open_document_from_bytes(&bytes, None).unwrap();
        let document = pdf_save::document_from_lopdf(&base, security).unwrap();
        Session {
            document,
            base,
            bytes,
            checkpoints: Checkpoints::default(),
        }
    }

    /// The runs of page 0 as a shell reads them now: pending edits overlaid.
    fn runs(&self) -> Vec<TextRun> {
        pdf_save::read_pending_page_content_of(
            &self.document,
            PageId(0),
            &self.base,
            ImportedSources::none(),
        )
        .unwrap()
        .text_runs
    }

    fn run(&self, text: &str) -> TextRun {
        self.runs()
            .into_iter()
            .find(|run| run.text == text)
            .unwrap_or_else(|| panic!("no run {text:?} in {:?}", self.runs()))
    }

    /// Records through the checkpointing path every shell uses.
    fn record(&mut self, command: Command) -> Result<(), pdf_save::RecordError> {
        pdf_save::checkpoint::record(self, command, |session, command| {
            record_content_command(
                &mut session.document,
                &session.base,
                ImportedSources::none(),
                command,
            )
        })
    }

    /// Records with no checkpoint to fall back on.
    fn record_plain(&mut self, command: Command) -> Result<(), pdf_save::RecordError> {
        record_content_command(
            &mut self.document,
            &self.base,
            ImportedSources::none(),
            command,
        )
    }

    fn saved(&self) -> Session {
        let bytes = save_document(SaveInput {
            document: &self.document,
            base: &self.base,
            original_bytes: Some(&self.bytes),
            intent: SaveIntent::Default,
            signatures: SignatureAcknowledgement::Unacknowledged,
            imported_sources: ImportedSources::none(),
        })
        .expect("the save is written");
        Session::open(bytes)
    }

    fn look(&self, text: &str) -> TextRunLook {
        pdf_save::text_run_look_of(
            &self.document,
            &self.base,
            ImportedSources::none(),
            &self.run(text),
        )
        .unwrap()
        .expect("a page with content")
        .look
    }
}

impl CheckpointSession for Session {
    type Base = (LopdfDocument, Vec<u8>);

    fn document(&self) -> &Document {
        &self.document
    }
    fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }
    fn base(&self) -> &LopdfDocument {
        &self.base
    }
    fn original_bytes(&self) -> Option<&[u8]> {
        Some(&self.bytes)
    }
    fn has_imported_sources(&self) -> bool {
        false
    }
    fn checkpoints(&self) -> &Checkpoints<Self::Base> {
        &self.checkpoints
    }
    fn checkpoints_mut(&mut self) -> &mut Checkpoints<Self::Base> {
        &mut self.checkpoints
    }
    fn checkpointed_base(&self, base: LopdfDocument, bytes: Vec<u8>) -> Self::Base {
        (base, bytes)
    }
    fn replace_base(&mut self, (base, bytes): Self::Base) -> Self::Base {
        let old_base = std::mem::replace(&mut self.base, base);
        let old_bytes = std::mem::replace(&mut self.bytes, bytes);
        (old_base, old_bytes)
    }
}

fn look(bold: bool, italic: bool, underline: bool) -> TextRunLook {
    TextRunLook {
        bold,
        italic,
        underline,
        strike: false,
    }
}

fn restyle(run: TextRun, from: TextRunLook, to: TextRunLook) -> Command {
    Command::RestyleTextRun {
        item: run,
        from,
        to,
    }
}

#[test]
fn a_bold_toggle_saves_and_reads_back_bold() {
    let mut session = Session::open(pdf_bytes(HELLO, true));
    let hello = session.run("Hello");

    session
        .record(restyle(
            hello,
            look(false, false, false),
            look(true, false, false),
        ))
        .unwrap();
    let saved = session.saved();

    assert_eq!(
        saved.run("Hello").resource_font_name,
        "F2",
        "the page's bold face"
    );
    assert!(saved.look("Hello").bold);
    assert!(!saved.look(" world").bold);
}

#[test]
fn a_second_toggle_of_the_same_run_amends_the_first() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    let hello = session.run("Hello");
    let plain = look(false, false, false);

    session
        .record(restyle(hello.clone(), plain, look(true, false, false)))
        .unwrap();
    session
        .record(restyle(
            session.run("Hello"),
            look(true, false, false),
            look(true, true, true),
        ))
        .unwrap();

    assert_eq!(
        session.document.pending_edits.entries(),
        [restyle(hello, plain, look(true, true, true))],
        "one step, from the look the file has to the last one asked for"
    );
    let saved = session.saved();
    assert_eq!(saved.look("Hello"), look(true, true, true));
    let lines = pdf_save::read_page_content(&saved.base, PageId(0))
        .unwrap()
        .paths;
    assert_eq!(lines.len(), 1, "underlined once");
}

#[test]
fn toggling_back_before_the_save_writes_nothing() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    let plain = look(false, false, false);
    let underlined = look(false, false, true);

    session
        .record(restyle(session.run("Hello"), plain, underlined))
        .unwrap();
    session
        .record(restyle(session.run("Hello"), underlined, plain))
        .unwrap();

    let saved = session.saved();
    assert_eq!(saved.look("Hello"), plain);
    assert!(pdf_save::read_page_content(&saved.base, PageId(0))
        .unwrap()
        .paths
        .is_empty());
}

#[test]
fn undo_takes_the_restyle_back_out_of_the_save() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    session
        .record(restyle(
            session.run("Hello"),
            look(false, false, false),
            look(true, false, false),
        ))
        .unwrap();

    assert!(pdf_save::checkpoint::undo(&mut session));

    assert!(!session.saved().look("Hello").bold);
}

#[test]
fn a_restyle_after_a_retype_of_the_same_run_saves_both() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    session
        .record(Command::ReplaceTextRunContent {
            item: session.run("Hello"),
            after: "Howdy".to_string(),
        })
        .unwrap();

    session
        .record(restyle(
            session.run("Howdy"),
            look(false, false, false),
            look(true, false, false),
        ))
        .unwrap();

    let saved = session.saved();
    assert!(saved.look("Howdy").bold);
}

#[test]
fn a_retype_after_a_synthesized_bold_saves_both() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    session
        .record(restyle(
            session.run("Hello"),
            look(false, false, false),
            look(true, false, false),
        ))
        .unwrap();

    session
        .record_plain(Command::ReplaceTextRunContent {
            item: session.run("Hello"),
            after: "Howdy".to_string(),
        })
        .expect("a stroked bold keeps the run where a retype finds it");

    let saved = session.saved();
    assert!(saved.look("Howdy").bold);
}

#[test]
fn a_retype_after_a_switch_to_a_sibling_goes_through_a_checkpoint() {
    let mut session = Session::open(pdf_bytes(HELLO, true));
    session
        .record(restyle(
            session.run("Hello"),
            look(false, false, false),
            look(true, false, false),
        ))
        .unwrap();
    let retype = Command::ReplaceTextRunContent {
        item: session.run("Hello"),
        after: "Howdy".to_string(),
    };

    assert!(
        session.clone_plain_record(retype.clone()).is_err(),
        "the run is no longer in its own font once the restyle is written"
    );
    session.record(retype).expect("retried after a checkpoint");

    let saved = session.saved();
    assert_eq!(saved.run("Howdy").resource_font_name, "F2");
    assert!(saved.look("Howdy").bold);
}

#[test]
fn bold_off_needs_a_regular_face_for_a_natively_bold_run() {
    let mut lone = Session::open(lone_bold_pdf());
    let refused = lone.record_plain(restyle(
        lone.run("Hello"),
        look(true, false, false),
        look(false, false, false),
    ));
    assert!(refused.is_err(), "un-bolding is never faked");
    assert!(lone.document.pending_edits.entries().is_empty());

    let mut paired = Session::open(pdf_bytes(b"BT /F2 12 Tf 72 700 Td (Hello) Tj ET", true));
    paired
        .record_plain(restyle(
            paired.run("Hello"),
            look(true, false, false),
            look(false, false, false),
        ))
        .expect("the page has the regular face");
    let saved = paired.saved();
    assert_eq!(saved.run("Hello").resource_font_name, "F1");
    assert!(!saved.look("Hello").bold);
}

/// A page whose only font is Helvetica-Bold.
fn lone_bold_pdf() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let stream = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F2 12 Tf 72 700 Td (Hello) Tj ET".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => stream,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F2" => font("Helvetica-Bold") } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => 1, "Kids" => vec![page.into()],
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn restyling_several_runs_is_one_batch_that_folds_when_repeated() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    let runs = session.runs();
    let plain = look(false, false, false);
    let batch = |from, to| {
        Command::Batch(
            runs.iter()
                .map(|run| restyle(run.clone(), from, to))
                .collect(),
        )
    };

    pdf_save::record_content_batch(
        &mut session.document,
        &session.base,
        ImportedSources::none(),
        batch(plain, look(true, false, false)),
    )
    .unwrap();
    pdf_save::record_content_batch(
        &mut session.document,
        &session.base,
        ImportedSources::none(),
        batch(look(true, false, false), look(true, true, false)),
    )
    .unwrap();

    assert_eq!(
        session.document.pending_edits.entries(),
        [batch(plain, look(true, true, false))]
    );
    let saved = session.saved();
    assert_eq!(saved.look("Hello"), look(true, true, false));
    assert_eq!(saved.look(" world"), look(true, true, false));
}

impl Session {
    /// Whether a plain record would take `command`, without recording it.
    fn clone_plain_record(&self, command: Command) -> Result<(), pdf_save::RecordError> {
        let mut document = self.document.clone();
        record_content_command(&mut document, &self.base, ImportedSources::none(), command)
    }
}

fn line_count(session: &Session) -> usize {
    pdf_save::read_page_content(&session.base, PageId(0))
        .unwrap()
        .paths
        .len()
}

#[test]
fn an_underline_reads_back_after_a_save_and_comes_off_after_the_reopen() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    session
        .record(restyle(
            session.run("Hello"),
            look(false, false, false),
            look(false, false, true),
        ))
        .unwrap();
    let mut reopened = session.saved();
    assert!(reopened.look("Hello").underline, "found in the saved file");

    reopened
        .record(restyle(
            reopened.run("Hello"),
            look(false, false, true),
            look(false, false, false),
        ))
        .unwrap();

    let saved = reopened.saved();
    assert!(!saved.look("Hello").underline);
    assert_eq!(line_count(&saved), 0, "the line is gone, not hidden");
}

#[test]
fn underlining_an_underlined_run_again_never_doubles_the_line() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    let underlined = look(false, false, true);
    session
        .record(restyle(
            session.run("Hello"),
            look(false, false, false),
            underlined,
        ))
        .unwrap();
    let mut reopened = session.saved();

    // A shell that did not read the line back would claim the run plain.
    reopened
        .record(restyle(
            reopened.run("Hello"),
            look(false, false, false),
            underlined,
        ))
        .unwrap();

    assert_eq!(line_count(&reopened.saved()), 1);
}

#[test]
fn an_underline_written_by_a_checkpoint_is_read_back_and_removable() {
    let mut session = Session::open(pdf_bytes(HELLO, true));
    session
        .record(restyle(
            session.run("Hello"),
            look(false, false, false),
            look(true, false, true),
        ))
        .unwrap();
    // A retype of a run switched to its bold face goes through a checkpoint,
    // which writes the underline into the new base.
    session
        .record(Command::ReplaceTextRunContent {
            item: session.run("Hello"),
            after: "Howdy".to_string(),
        })
        .unwrap();
    assert!(session.look("Howdy").underline, "read from the new base");

    session
        .record(restyle(
            session.run("Howdy"),
            look(true, false, true),
            look(true, false, false),
        ))
        .unwrap();

    let saved = session.saved();
    assert_eq!(saved.look("Howdy"), look(true, false, false));
    assert_eq!(line_count(&saved), 0);
}

#[test]
fn a_moved_underlined_run_keeps_one_line_under_it() {
    let mut session = Session::open(pdf_bytes(HELLO, false));
    session
        .record(restyle(
            session.run("Hello"),
            look(false, false, false),
            look(false, false, true),
        ))
        .unwrap();
    let mut reopened = session.saved();
    let run = reopened.run("Hello");
    let to = pdf_document::Rect {
        x: run.bbox.x + 40.0,
        y: run.bbox.y - 200.0,
        ..run.bbox
    };
    reopened
        .record(Command::MoveTextRun { item: run, to })
        .unwrap();

    let saved = reopened.saved();
    assert!(saved.look("Hello").underline, "the line went with the run");
    assert_eq!(line_count(&saved), 1);
}
