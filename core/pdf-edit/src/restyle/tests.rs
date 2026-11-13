use super::*;
use crate::fixture::{document_with_content, helvetica_resources};
use crate::parse::{read_page_object_content, tokenize};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use pdf_document::{PageContent, PageId, TextRun, TextRunLook};

const HELLO: &[u8] = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ( world) Tj ET";

fn look(bold: bool, italic: bool) -> TextRunLook {
    TextRunLook {
        bold,
        italic,
        ..TextRunLook::default()
    }
}

fn underlined() -> TextRunLook {
    TextRunLook {
        underline: true,
        ..TextRunLook::default()
    }
}

fn standard_font(base_font: &str) -> Object {
    Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => base_font,
        "Encoding" => "WinAnsiEncoding",
    })
}

/// `/F1` plus each `(name, base font)` as a WinAnsi standard font.
fn fonts(extra: &[(&str, &str)]) -> Dictionary {
    let mut resources = helvetica_resources();
    let Ok(Object::Dictionary(mut fonts)) = resources.get(b"Font").cloned() else {
        unreachable!("helvetica_resources has a font dictionary");
    };
    for (name, base_font) in extra {
        fonts.set(*name, standard_font(base_font));
    }
    resources.set("Font", fonts);
    resources
}

fn content(document: &Document, page: ObjectId) -> PageContent {
    read_page_object_content(document, page, PageId(0)).expect("the page parses")
}

fn first_run(document: &Document, page: ObjectId) -> TextRun {
    content(document, page).text_runs.remove(0)
}

/// Every operator of every content stream on the page, in order.
fn operators(document: &Document, page: ObjectId) -> Vec<String> {
    crate::parse::page_streams(document, document.get_dictionary(page).expect("page"))
        .expect("streams")
        .iter()
        .flat_map(|stream| tokenize(&stream.bytes).expect("tokenizes"))
        .map(|operation| operation.operator)
        .collect()
}

fn page_text(document: &Document, page: ObjectId) -> String {
    crate::parse::page_streams(document, document.get_dictionary(page).expect("page"))
        .expect("streams")
        .iter()
        .map(|stream| String::from_utf8_lossy(&stream.bytes).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

fn restyle(document: &mut Document, page: ObjectId, from: TextRunLook, to: TextRunLook) {
    let run = first_run(document, page);
    restyle_text_run(document, page, &run, from, to).expect("the restyle is written");
}

#[test]
fn bold_switches_to_a_bold_sibling_the_page_already_has() {
    let (mut document, page) = document_with_content(HELLO, fonts(&[("F2", "Helvetica-Bold")]));

    restyle(&mut document, page, look(false, false), look(true, false));

    let runs = content(&document, page).text_runs;
    assert_eq!(runs[0].text, "Hello");
    assert_eq!(runs[0].resource_font_name, "F2", "painted in the bold face");
    assert_eq!(
        runs[1].resource_font_name, "F1",
        "and the run after it in its own font again"
    );
    assert!(!operators(&document, page).contains(&"Tr".to_string()));
    assert_eq!(
        text_run_look(&document, page, &runs[0]).unwrap().look,
        look(true, false)
    );
}

#[test]
fn the_sibling_keeps_the_runs_italic_ness() {
    let (mut document, page) = document_with_content(
        b"BT /F3 12 Tf 100 700 Td (Hello) Tj ET",
        fonts(&[
            ("F2", "Helvetica-Bold"),
            ("F3", "Helvetica-Oblique"),
            ("F4", "Helvetica-BoldOblique"),
        ]),
    );

    restyle(&mut document, page, look(false, true), look(true, true));

    assert_eq!(first_run(&document, page).resource_font_name, "F4");
}

#[test]
fn bold_is_synthesized_when_no_sibling_exists() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    let before = first_run(&document, page);

    restyle(&mut document, page, look(false, false), look(true, false));

    let text = page_text(&document, page);
    assert!(
        text.contains("2 Tr"),
        "glyphs are filled and stroked: {text}"
    );
    assert!(text.contains("0 G"), "stroked in the fill colour: {text}");
    assert!(
        text.contains("0.36 w"),
        "about 3% of the size thick: {text}"
    );
    let runs = content(&document, page).text_runs;
    assert_eq!(runs[0], before, "the run is still the same run");
    assert_eq!(
        text_run_look(&document, page, &runs[0]).unwrap().look,
        look(true, false)
    );
    assert!(
        !text_run_look(&document, page, &runs[1]).unwrap().look.bold,
        "the run after it is painted as before"
    );
}

#[test]
fn a_sibling_that_cannot_show_the_text_is_passed_over() {
    let (mut document, page) =
        document_with_content(HELLO, fonts(&[("F2", "PLACEH+Helvetica-Bold")]));
    narrow_to_h(&mut document, page, "F2");

    restyle(&mut document, page, look(false, false), look(true, false));

    let text = page_text(&document, page);
    assert!(!text.contains("/F2"), "the subset lacks `ello`: {text}");
    assert!(text.contains("2 Tr"), "so bold is synthesized: {text}");
}

/// Turns the standard font at `name` into a TrueType subset whose CMap only
/// vouches for `H`.
fn narrow_to_h(document: &mut Document, page: ObjectId, name: &str) {
    let cmap = document.add_object(lopdf::Stream::new(
        dictionary! {},
        b"1 begincodespacerange <00> <7F> endcodespacerange\n1 beginbfchar <48> <0048> endbfchar"
            .to_vec(),
    ));
    let widths: Vec<Object> = (0..=0x7F).map(|_| 600.into()).collect();
    let resources = document
        .get_dictionary_mut(page)
        .unwrap()
        .get_mut(b"Resources")
        .unwrap()
        .as_dict_mut()
        .unwrap();
    let fonts = resources.get_mut(b"Font").unwrap().as_dict_mut().unwrap();
    let font = fonts
        .get_mut(name.as_bytes())
        .unwrap()
        .as_dict_mut()
        .unwrap();
    font.set("Subtype", "TrueType");
    font.remove(b"Encoding");
    font.set("FirstChar", 0);
    font.set("LastChar", 0x7F);
    font.set("Widths", widths);
    font.set("ToUnicode", cmap);
}

#[test]
fn italic_is_a_shear_around_the_runs_origin_when_no_sibling_exists() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    let before = content(&document, page).text_runs;

    restyle(&mut document, page, look(false, false), look(false, true));

    let after = content(&document, page).text_runs;
    assert_eq!(after[0].text, "Hello");
    assert!(
        (after[0].bbox.y - before[0].bbox.y).abs() < 1e-6,
        "the baseline stays"
    );
    assert!(
        (after[0].bbox.x - before[0].bbox.x).abs() < 1.0,
        "the origin stays, the box only widens by the slant"
    );
    assert!(
        (after[1].bbox.x - before[1].bbox.x).abs() < 1e-6,
        "the text after it does not move"
    );
    assert_eq!(after[1].bbox, before[1].bbox, "nor slant");
    assert_eq!(
        text_run_look(&document, page, &after[0]).unwrap().look,
        look(false, true)
    );
}

#[test]
fn italic_switches_to_an_italic_sibling() {
    let (mut document, page) = document_with_content(HELLO, fonts(&[("F2", "Helvetica-Oblique")]));

    restyle(&mut document, page, look(false, false), look(false, true));

    assert_eq!(first_run(&document, page).resource_font_name, "F2");
}

#[test]
fn underline_is_a_thin_filled_box_under_the_run_in_its_fill_colour() {
    let (mut document, page) = document_with_content(
        b"BT 1 0 0 rg /F1 12 Tf 100 700 Td (Hello) Tj ET",
        helvetica_resources(),
    );
    let run = first_run(&document, page);

    restyle(&mut document, page, TextRunLook::default(), underlined());

    let content = content(&document, page);
    assert_eq!(content.text_runs[0], run, "the run is untouched");
    let [line] = content.paths.as_slice() else {
        panic!("one line is drawn: {:?}", content.paths);
    };
    assert!(
        (line.bbox.x - run.bbox.x).abs() < 1e-6,
        "from the run's start"
    );
    assert!(
        (line.bbox.width - run.bbox.width).abs() < 1e-6,
        "across its advance"
    );
    assert!(line.bbox.y < 700.0, "below the baseline");
    assert!(line.bbox.y + line.bbox.height < 700.0);
    assert!(page_text(&document, page).contains("1 0 0 rg"), "in red");
}

#[test]
fn strikethrough_crosses_the_run_above_the_baseline() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());

    restyle(
        &mut document,
        page,
        TextRunLook::default(),
        TextRunLook {
            strike: true,
            ..TextRunLook::default()
        },
    );

    let [line] = content(&document, page).paths.try_into().expect("one line");
    assert!(
        line.bbox.y > 700.0 && line.bbox.y < 706.0,
        "{:?}",
        line.bbox
    );
}

#[test]
fn bold_off_switches_a_natively_bold_run_to_its_regular_sibling() {
    let (mut document, page) = document_with_content(
        b"BT /F2 12 Tf 100 700 Td (Hello) Tj ET",
        fonts(&[("F2", "Helvetica-Bold")]),
    );
    let run = first_run(&document, page);
    let options = text_run_look(&document, page, &run).unwrap();
    assert!(options.look.bold && options.can_remove_bold);

    restyle(&mut document, page, look(true, false), look(false, false));

    assert_eq!(first_run(&document, page).resource_font_name, "F1");
}

#[test]
fn a_natively_bold_run_without_a_regular_sibling_cannot_be_unbolded() {
    let resources = dictionary! {
        "Font" => dictionary! { "F2" => standard_font("Helvetica-Bold") },
    };
    let (mut document, page) =
        document_with_content(b"BT /F2 12 Tf 100 700 Td (Hello) Tj ET", resources);
    let run = first_run(&document, page);

    let options = text_run_look(&document, page, &run).unwrap();
    assert!(options.look.bold);
    assert!(!options.can_remove_bold, "faking un-bold is not offered");
    let untouched = document.clone();
    let refused = restyle_text_run(
        &mut document,
        page,
        &run,
        look(true, false),
        look(false, false),
    );
    assert!(matches!(refused, Err(EditError::StyleNotRemovable { .. })));
    assert_eq!(document.objects, untouched.objects, "nothing is written");
}

#[test]
fn bold_off_cancels_a_stroked_bold_already_in_the_file() {
    let (mut document, page) = document_with_content(
        b"BT 2 Tr /F1 12 Tf 100 700 Td (Hello) Tj ET",
        helvetica_resources(),
    );
    let run = first_run(&document, page);
    let options = text_run_look(&document, page, &run).unwrap();
    assert!(options.look.bold && options.can_remove_bold);

    restyle(&mut document, page, look(true, false), look(false, false));

    let run = first_run(&document, page);
    assert!(!text_run_look(&document, page, &run).unwrap().look.bold);
}

#[test]
fn italic_off_straightens_a_shear_already_in_the_file() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    restyle(&mut document, page, look(false, false), look(false, true));
    let run = first_run(&document, page);
    assert!(
        text_run_look(&document, page, &run)
            .unwrap()
            .can_remove_italic
    );

    restyle(&mut document, page, look(false, true), look(false, false));

    let run = first_run(&document, page);
    assert!(!text_run_look(&document, page, &run).unwrap().look.italic);
}

#[test]
fn bold_and_italic_together_without_siblings_are_both_synthesized() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());

    restyle(&mut document, page, look(false, false), look(true, true));

    let run = first_run(&document, page);
    assert_eq!(
        text_run_look(&document, page, &run).unwrap().look,
        look(true, true)
    );
    let after = content(&document, page).text_runs;
    assert!(!text_run_look(&document, page, &after[1]).unwrap().look.bold);
}

#[test]
fn italic_with_a_font_switch_keeps_the_next_run_after_the_new_advance() {
    // Bold comes from a sibling, italic from a shear: the restore after the
    // sheared show has to land where the bold glyphs ended.
    let (mut document, page) = document_with_content(HELLO, fonts(&[("F2", "Helvetica-Bold")]));
    let plain_bold = {
        let (mut bold, page) = document_with_content(HELLO, fonts(&[("F2", "Helvetica-Bold")]));
        restyle(&mut bold, page, look(false, false), look(true, false));
        content(&bold, page).text_runs[1].bbox
    };

    restyle(&mut document, page, look(false, false), look(true, true));

    let after = content(&document, page).text_runs;
    assert_eq!(after[0].resource_font_name, "F2");
    assert!((after[1].bbox.x - plain_bold.x).abs() < 1e-6);
}

#[test]
fn a_restyle_that_changes_nothing_writes_nothing() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    let untouched = document.clone();

    restyle(&mut document, page, look(true, false), look(false, false));

    assert_eq!(document.objects, untouched.objects);
}

#[test]
fn a_run_shown_with_a_quote_is_sheared_without_a_second_line_feed() {
    let (mut document, page) = document_with_content(
        b"BT /F1 12 Tf 14 TL 100 700 Td (one) Tj (two) ' ET",
        helvetica_resources(),
    );
    let before = content(&document, page).text_runs[1].bbox;
    let second = content(&document, page).text_runs.remove(1);

    restyle_text_run(
        &mut document,
        page,
        &second,
        look(false, false),
        look(false, true),
    )
    .expect("written");

    let after = content(&document, page).text_runs[1].bbox;
    assert!(
        (after.y - before.y).abs() < 1e-6,
        "still on the second line"
    );
}

#[test]
fn a_pasted_run_restyles_like_any_other() {
    let (source, source_page) = document_with_content(HELLO, helvetica_resources());
    let run = first_run(&source, source_page);
    let clip = crate::copy_selection(
        &source,
        source_page,
        &crate::Selection {
            runs: vec![run.clone()],
            ..crate::Selection::default()
        },
    )
    .expect("copied");
    let (mut document, page) = document_with_content(b"", helvetica_resources());
    crate::paste_clip(&mut document, page, &clip, run.bbox).expect("pasted");
    let pasted = first_run(&document, page);

    restyle_text_run(
        &mut document,
        page,
        &pasted,
        look(false, false),
        look(true, true),
    )
    .expect("written");

    let run = first_run(&document, page);
    assert_eq!(run.text, "Hello");
    assert_eq!(
        text_run_look(&document, page, &run).unwrap().look,
        look(true, true)
    );
}

#[test]
fn family_names_drop_subset_tags_and_style_suffixes() {
    for (name, family) in [
        ("ABCDEF+Arial-BoldMT", "arial"),
        ("ArialMT", "arial"),
        ("Arial,BoldItalic", "arial"),
        ("TimesNewRomanPS-BoldItalicMT", "timesnewroman"),
        ("TimesNewRomanPSMT", "timesnewroman"),
        ("Helvetica-Oblique", "helvetica"),
        ("Courier-BoldOblique", "courier"),
        ("Times-Roman", "times"),
        ("Times-Italic", "times"),
        ("DejaVuSans-Bold", "dejavusans"),
    ] {
        assert_eq!(sibling::family_of(name), family, "{name}");
    }
}

#[test]
fn the_written_stream_stays_tokenizable_and_balanced() {
    let (mut document, page) = document_with_content(HELLO, helvetica_resources());
    restyle(
        &mut document,
        page,
        TextRunLook::default(),
        TextRunLook {
            bold: true,
            italic: true,
            underline: true,
            strike: true,
        },
    );
    let operators = operators(&document, page);
    let count = |op: &str| operators.iter().filter(|o| *o == op).count();
    assert_eq!(count("q"), count("Q"));
    assert_eq!(count("BT"), count("ET"));
}
