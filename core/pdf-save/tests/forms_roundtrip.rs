//! T-145: form fields through the whole save pipeline, both writers.
//!
//! `forms.rs`'s unit tests prove each object is built correctly; these prove
//! the document that comes back out of a real save still describes the same
//! form. Two things they cover that no unit test structurally can:
//!
//! - **Both writers agree.** A form edit never forces a full rewrite
//!   (batch decision 6), so an opened file takes the incremental path and a
//!   freshly authored one takes the full-rewrite path. The four field kinds
//!   have to survive both, and the round trip is what says so — the writers
//!   share `write_form_fields` but not the file structure it lands in.
//! - **A fill is additive.** Filling in a field of a PDF this workspace did
//!   not write must change `/V` and `/AP` and leave every original byte in
//!   place as the prefix of the result, because that is the only reason an
//!   existing signature survives one.
//!
//! The foreign document is `tests/fixtures/forms/reportlab_acroform.pdf`
//! (T-144). The last two tests here are `#[ignore]`d and write a
//! caller-owned file for `tools/pypdf-validation/validate_forms_roundtrip.py`
//! (T-146) to check with an independent library — the same split
//! `content_edit_roundtrip.rs` and `metadata_roundtrip.rs` already use.

use pdf_document::{
    Color, Command, Document, FieldValue, FontFamily, FormField, FormFieldId, FormFieldKind,
    PageId, RadioOption, Rect, TextStyle,
};
use pdf_save::{
    document_from_lopdf, save_document, SaveInput, SaveIntent, SignatureAcknowledgement,
};

fn temp_pdf_path(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pdf-save-forms-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("doc.pdf")
}

fn apply_command(document: &mut Document, command: Command) {
    let mut log = std::mem::take(&mut document.pending_edits);
    assert!(log.apply(document, command), "command should apply");
    document.pending_edits = log;
}

fn save(
    document: &Document,
    base: &pdf_manip::LopdfDocument,
    original_bytes: Option<&[u8]>,
) -> Vec<u8> {
    save_document(SaveInput {
        document,
        base,
        original_bytes,
        intent: SaveIntent::Default,
        signatures: SignatureAcknowledgement::Unacknowledged,
        imported_sources: pdf_save::ImportedSources::none(),
    })
    .expect("save should succeed")
}

fn reopen(bytes: &[u8]) -> Document {
    let (base, security) = pdf_manip::open_document_from_bytes(bytes, None).expect("reopen");
    document_from_lopdf(&base, security).expect("rebuild the model")
}

fn external_acroform_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("forms")
        .join("reportlab_acroform.pdf")
}

fn style(font: FontFamily, size_pt: f64) -> TextStyle {
    TextStyle {
        font,
        size_pt,
        color: Color { r: 0, g: 0, b: 0 },
    }
}

/// A style with a colour that is not the default black — the only half of a
/// `/Btn` field's `TextStyle` that anything downstream consumes, and so the
/// only half a round trip can be asked to carry (T-205).
fn colored_style(font: FontFamily, size_pt: f64, color: Color) -> TextStyle {
    TextStyle {
        font,
        size_pt,
        color,
    }
}

const CHECKBOX_RED: Color = Color { r: 204, g: 0, b: 0 };
const RADIO_BLUE: Color = Color {
    r: 0,
    g: 64,
    b: 192,
};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// One field of each kind, with values set, deliberately distinct in every
/// attribute the model carries so a round trip that silently normalised one
/// of them would show.
fn the_four_kinds() -> Vec<FormField> {
    vec![
        pdf_form::text_field(
            FormFieldId(0),
            PageId(0),
            "applicant",
            rect(72.0, 700.0, 240.0, 22.0),
            style(FontFamily::Helvetica, 12.0),
            true,
            Some(64),
        ),
        pdf_form::checkbox(
            FormFieldId(1),
            PageId(0),
            "agrees",
            rect(72.0, 660.0, 18.0, 18.0),
            colored_style(FontFamily::TimesRoman, 10.0, CHECKBOX_RED),
        ),
        pdf_form::radio_group(
            FormFieldId(2),
            PageId(0),
            "plan",
            rect(72.0, 600.0, 78.0, 18.0),
            colored_style(FontFamily::Helvetica, 11.0, RADIO_BLUE),
            vec![
                RadioOption {
                    export_value: "basic".to_string(),
                    rect: rect(72.0, 600.0, 18.0, 18.0),
                },
                RadioOption {
                    export_value: "pro".to_string(),
                    rect: rect(132.0, 600.0, 18.0, 18.0),
                },
            ],
        ),
        pdf_form::dropdown(
            FormFieldId(3),
            PageId(0),
            "country",
            rect(72.0, 550.0, 240.0, 22.0),
            style(FontFamily::Courier, 9.0),
            vec![
                "Argentina".to_string(),
                "Uruguay".to_string(),
                "Chile".to_string(),
            ],
            false,
        ),
    ]
}

fn the_four_values() -> Vec<FieldValue> {
    vec![
        FieldValue::Text("Ada Lovelace".to_string()),
        FieldValue::Checked(true),
        FieldValue::Choice(Some("pro".to_string())),
        FieldValue::Choice(Some("Uruguay".to_string())),
    ]
}

/// Queues `AddFormField` for each of the four kinds, then a `SetFieldValue`
/// for each — two steps because a builder always produces an unset field
/// (that is `builders`' documented contract) and the value is what a fill
/// writes.
fn author_the_four_kinds(document: &mut Document) {
    for field in the_four_kinds() {
        apply_command(document, Command::AddFormField(field));
    }
    for (field, value) in the_four_kinds().into_iter().zip(the_four_values()) {
        apply_command(
            document,
            Command::SetFieldValue {
                id: field.id,
                from: field.value,
                to: value,
            },
        );
    }
}

/// What survives a save is the *form*, not the session. Three things are
/// deliberately left out of the comparison:
///
/// - `id`, because `read_form_fields` reassigns ids sequentially in
///   `/Fields` order (its own documented contract). The order is what is
///   asserted instead, by comparing the sequences position by position.
/// - `origin`, because every field read back out of a file is
///   `Existing(oid)` by definition — that is the whole point of the entry —
///   and the object id it names cannot be predicted from the input.
/// - `style`, because how much of it survives depends on the kind, which
///   makes a whole-struct comparison the wrong shape rather than a weaker
///   one. `Tx` and `Ch` keep all three attributes
///   ([`the_variable_text_kinds_keep_their_style`]); a `/Btn` keeps its
///   colour and only its colour, by design
///   ([`a_buttons_color_survives_the_round_trip`] and
///   [`a_buttons_font_and_size_do_not_survive_and_are_not_meant_to`]).
///   Those three tests compare it, per kind, where the contract differs.
#[derive(Debug, PartialEq)]
struct Comparable {
    name: String,
    page: PageId,
    rect: Rect,
    value: FieldValue,
    kind: FormFieldKind,
}

fn comparable(field: &FormField) -> Comparable {
    Comparable {
        name: field.name.clone(),
        page: field.page,
        rect: field.rect,
        value: field.value.clone(),
        kind: field.kind.clone(),
    }
}

fn comparable_set(document: &Document) -> Vec<Comparable> {
    document.form_fields.iter().map(comparable).collect()
}

fn field_named<'a>(document: &'a Document, name: &str) -> &'a FormField {
    document
        .form_fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("no field named {name:?}"))
}

/// A freshly authored document has no `original_bytes`, so this is the
/// full-rewrite writer.
/// A one-page document authored from nothing, carrying all four kinds with
/// their values set. No `original_bytes`, so every save of it goes down the
/// full-rewrite writer.
fn an_authored_form() -> (Document, pdf_manip::LopdfDocument) {
    let base = pdf_manip::create_blank_document(
        pdf_document::PageSize::A4,
        pdf_document::Orientation::Portrait,
    );
    let base = pdf_manip::insert_blank_page(
        &base,
        0,
        pdf_document::PageSize::A4,
        pdf_document::Orientation::Portrait,
    )
    .expect("one page");
    let mut document = document_from_lopdf(&base, None).expect("model");
    author_the_four_kinds(&mut document);
    (document, base)
}

#[test]
fn the_four_kinds_survive_a_full_rewrite_with_their_values() {
    let (document, base) = an_authored_form();
    let expected = comparable_set(&document);

    let reloaded = reopen(&save(&document, &base, None));

    assert_eq!(comparable_set(&reloaded), expected);
}

/// The same four fields down the other writer. An opened file with
/// `original_bytes` and no structural page change takes the incremental
/// path, which is where a form edit belongs (batch decision 6).
#[test]
fn the_four_kinds_survive_an_incremental_save_with_their_values() {
    let mut fixture = gen_fixtures::forms::build_acroform_document();
    let path = temp_pdf_path("incremental-four-kinds");
    fixture.save(&path).expect("write fixture");
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open fixture");
    let mut document = document_from_lopdf(&base, security).expect("model");

    // The fixture's own two fields are already in the set; the four
    // authored ones must not collide with their ids.
    let next_id = document
        .form_fields
        .iter()
        .map(|field| field.id.0)
        .max()
        .map_or(0, |highest| highest + 1);
    for (offset, field) in the_four_kinds().into_iter().enumerate() {
        let mut field = field;
        field.id = FormFieldId(next_id + offset as u64);
        let before = field.value.clone();
        let value = the_four_values()[offset].clone();
        apply_command(&mut document, Command::AddFormField(field.clone()));
        apply_command(
            &mut document,
            Command::SetFieldValue {
                id: field.id,
                from: before,
                to: value,
            },
        );
    }
    let expected = comparable_set(&document);

    let saved = save(&document, &base, Some(&original_bytes));

    assert!(
        saved.starts_with(&original_bytes),
        "a form edit stays on the incremental writer"
    );
    assert_eq!(comparable_set(&reopen(&saved)), expected);
}

/// Style, for the two kinds whose `/DA` the writer actually emits. A `/DA`
/// is a *variable text* field attribute (ISO 32000-1 table 222), and `Tx`
/// and `Ch` are the two variable-text kinds — so this is the half of the
/// model the format is built to carry, and it must come back exactly.
#[test]
fn the_variable_text_kinds_keep_their_style() {
    let (document, base) = an_authored_form();

    let reloaded = reopen(&save(&document, &base, None));

    assert_eq!(
        field_named(&reloaded, "applicant").style,
        style(FontFamily::Helvetica, 12.0)
    );
    assert_eq!(
        field_named(&reloaded, "country").style,
        style(FontFamily::Courier, 9.0),
        "a non-default family really came back, rather than the default          happening to match"
    );
}

/// A `/Btn` field's **colour** survives the round trip (T-205), because it
/// is the one half of a button's `TextStyle` that anything consumes:
/// `appearance::build_field_appearance` paints the ZapfDingbats check mark
/// and the radio dot with it and bakes the result into `/AP`. Before T-205
/// the writer emitted `/DA` for `Tx` and `Ch` only, so a red check was drawn
/// red, saved red, and read back black — and the GTK4 inspector, which
/// offers colour for any field with no per-kind gating, then showed a colour
/// the file was not painting.
///
/// The `/DA` written for a button is Acrobat's own shape,
/// `/ZaDb 0 Tf <r g b> rg`: the resource a button's appearance actually
/// draws from, at the auto size, rather than this crate's `format_da` — a
/// downstream tool that regenerates the appearance from `/DA` must find
/// ZapfDingbats there, not `/Helv`.
#[test]
fn a_buttons_color_survives_the_round_trip() {
    let (document, base) = an_authored_form();
    assert_eq!(
        field_named(&document, "agrees").style.color,
        CHECKBOX_RED,
        "the authored checkbox really does carry a non-default colour"
    );

    let reloaded = reopen(&save(&document, &base, None));

    assert_eq!(field_named(&reloaded, "agrees").style.color, CHECKBOX_RED);
    assert_eq!(field_named(&reloaded, "plan").style.color, RADIO_BLUE);
}

/// The other half of T-205's decision, asserted so it is a contract rather
/// than an accident: a button's `font` and `size_pt` do **not** come back,
/// and must not. `/ZaDb 0 Tf` states the only font a button's appearance
/// ever draws with and leaves the size to the viewer, so there is nowhere in
/// the file for the user's family and point size to live — and nothing would
/// read them if there were (`build_field_appearance` sizes the glyph from
/// the control's own rect). They default on the way back in.
#[test]
fn a_buttons_font_and_size_do_not_survive_and_are_not_meant_to() {
    let (document, base) = an_authored_form();
    assert_eq!(
        (
            field_named(&document, "agrees").style.font,
            field_named(&document, "agrees").style.size_pt
        ),
        (FontFamily::TimesRoman, 10.0),
        "the authored checkbox really does carry a non-default family and size"
    );

    let reloaded = reopen(&save(&document, &base, None));

    let checkbox = field_named(&reloaded, "agrees").style;
    assert_eq!(
        (checkbox.font, checkbox.size_pt),
        (FontFamily::Helvetica, 12.0)
    );
    let radio = field_named(&reloaded, "plan").style;
    assert_eq!((radio.font, radio.size_pt), (FontFamily::Helvetica, 12.0));
}

/// The fill case, on a document this workspace did not write: change one
/// text field's `/V`, save incrementally, and the original bytes must still
/// be the prefix of the result. That prefix is the only reason a signature
/// already in the file survives a fill.
#[test]
fn filling_a_foreign_form_appends_without_rewriting_the_original_bytes() {
    let path = external_acroform_path();
    let original_bytes = std::fs::read(&path).expect("read the committed fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open the fixture");
    let mut document = document_from_lopdf(&base, security).expect("model");

    let target = document
        .form_fields
        .iter()
        .find(|field| field.name == "notes")
        .cloned()
        .expect("the fixture has an empty multiline field");
    assert_eq!(target.value, FieldValue::Text(String::new()));

    apply_command(
        &mut document,
        Command::SetFieldValue {
            id: target.id,
            from: target.value.clone(),
            to: FieldValue::Text("Filled by Vitela".to_string()),
        },
    );
    let saved = save(&document, &base, Some(&original_bytes));

    assert!(
        saved.starts_with(&original_bytes),
        "every original byte is still there, in place, as the prefix"
    );
    assert!(
        saved.len() > original_bytes.len(),
        "and something was added"
    );

    let reloaded = reopen(&saved);
    let read_back = reloaded
        .form_fields
        .iter()
        .find(|field| field.name == "notes")
        .expect("the field is still there");
    assert_eq!(
        read_back.value,
        FieldValue::Text("Filled by Vitela".to_string())
    );
    // Its neighbours came through untouched, including the radio group
    // whose widget kids restate `/FT` (T-144's regression).
    let names: Vec<_> = reloaded
        .form_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["full_name", "notes", "subscribe", "plan", "country"]
    );
}

/// `/V` and `/AP` are the two entries a fill has to move, and a checkbox is
/// where they are both visible: its value is a `/Name` and its appearance is
/// selected by `/AS` naming one of `/AP` `/N`'s states.
#[test]
fn filling_a_checkbox_moves_both_its_value_and_its_appearance_state() {
    let mut fixture = gen_fixtures::forms::build_acroform_document();
    let path = temp_pdf_path("checkbox-fill");
    fixture.save(&path).expect("write fixture");
    let original_bytes = std::fs::read(&path).expect("read fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open fixture");
    let mut document = document_from_lopdf(&base, security).expect("model");

    let target = document
        .form_fields
        .iter()
        .find(|field| field.name == gen_fixtures::forms::CHECKBOX_FIELD_NAME)
        .cloned()
        .expect("the fixture has a checkbox");
    assert_eq!(target.value, FieldValue::Checked(false));

    apply_command(
        &mut document,
        Command::SetFieldValue {
            id: target.id,
            from: target.value.clone(),
            to: FieldValue::Checked(true),
        },
    );
    let saved = save(&document, &base, Some(&original_bytes));

    assert!(saved.starts_with(&original_bytes));

    let (reloaded_base, _) =
        pdf_manip::open_document_from_bytes(&saved, None).expect("reopen the saved bytes");
    let lopdf = reloaded_base.as_lopdf();
    let widget = lopdf
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .find(|dict| {
            dict.get(b"T")
                .and_then(|name| name.as_str())
                .map(|name| name == gen_fixtures::forms::CHECKBOX_FIELD_NAME.as_bytes())
                .unwrap_or(false)
        })
        .expect("the checkbox is still addressable by its /T");

    let name_of = |key: &[u8]| {
        widget
            .get(key)
            .and_then(|object| object.as_name())
            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
            .unwrap_or_else(|error| {
                panic!("{} should be a name: {error}", String::from_utf8_lossy(key))
            })
    };
    assert_eq!(
        name_of(b"V"),
        gen_fixtures::forms::CHECKBOX_ON_STATE,
        "/V moved to the on-state"
    );
    assert_eq!(
        name_of(b"AS"),
        gen_fixtures::forms::CHECKBOX_ON_STATE,
        "/AS follows it, so a viewer paints the on appearance"
    );
    assert!(
        widget.has(b"AP"),
        "and the appearance dictionary is still there to select from"
    );
}

#[test]
#[ignore = "writes a caller-owned file for the standalone pypdf validator"]
fn write_pypdf_validation_output_for_authored_form() {
    let output = caller_owned_output();

    let (document, base) = an_authored_form();

    std::fs::write(&output, save(&document, &base, None)).expect("write the caller-owned output");
}

#[test]
#[ignore = "writes a caller-owned file for the standalone pypdf validator"]
fn write_pypdf_validation_output_for_filled_foreign_form() {
    let output = caller_owned_output();

    let path = external_acroform_path();
    let original_bytes = std::fs::read(&path).expect("read the committed fixture");
    let (base, security) = pdf_manip::open_document(&path, None).expect("open the fixture");
    let mut document = document_from_lopdf(&base, security).expect("model");

    let notes = document
        .form_fields
        .iter()
        .find(|field| field.name == "notes")
        .cloned()
        .expect("the fixture has an empty multiline field");
    apply_command(
        &mut document,
        Command::SetFieldValue {
            id: notes.id,
            from: notes.value,
            to: FieldValue::Text("Filled by Vitela".to_string()),
        },
    );
    let country = document
        .form_fields
        .iter()
        .find(|field| field.name == "country")
        .cloned()
        .expect("the fixture has a dropdown");
    apply_command(
        &mut document,
        Command::SetFieldValue {
            id: country.id,
            from: country.value,
            to: FieldValue::Choice(Some("Chile".to_string())),
        },
    );

    std::fs::write(&output, save(&document, &base, Some(&original_bytes)))
        .expect("write the caller-owned output");
}

fn caller_owned_output() -> std::path::PathBuf {
    let output = std::env::var_os("PDF_FORMS_VALIDATION_OUTPUT")
        .map(std::path::PathBuf::from)
        .expect("PDF_FORMS_VALIDATION_OUTPUT must name a caller-owned output file");
    assert!(
        !output.exists(),
        "refusing to overwrite caller-owned output"
    );
    std::fs::create_dir_all(output.parent().expect("output must have a parent"))
        .expect("create caller-owned output parent");
    output
}
