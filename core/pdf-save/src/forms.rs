//! Writes `pdf_document::FormField`s into a real lopdf object graph (T-138),
//! the form-field counterpart of [`crate::annotations`].
//!
//! Reuses [`crate::annotations::ObjectSink`] rather than adding a parallel
//! trait: `page_dict_mut` is not actually page-specific in either
//! implementation — it is a generic "get this object's dictionary, cloning
//! it into the new revision first if the writer is incremental" — so it
//! doubles here as the mutator for the `/AcroForm` dict, the catalog dict,
//! and an *existing* field or radio-kid dict.
//!
//! ## Why an existing field's update never needs a separate read of the base
//!
//! [`crate::strategy::save_full_rewrite`]'s `working` document starts as a
//! full clone of the base (`replay_page_ops`), and
//! `IncrementalDocument::page_dict_mut` clones an object from the prior
//! revision into the new one on first touch. Either way, `sink.page_dict_mut
//! (existing_oid)` already returns the field's *current* content — there is
//! no second "original" to fetch separately, unlike
//! `bridge::page_annotation_objects` reading `input.base` for annotations
//! (which the sink never holds a pre-populated view of on the incremental
//! path, because annotations are additive-only there).
//!
//! ## New vs. existing: what gets appended, and what doesn't
//!
//! A **new** field's widget object id must be appended to both its page's
//! `/Annots` and `/AcroForm /Fields` — nothing referenced it before. An
//! **existing** field's object id is already correctly referenced by both
//! (from the file this session opened) and keeps that same id (decision 5:
//! clone-and-modify) — so updating it touches only the object itself, never
//! either array. A radio group's parent field dict is the one exception:
//! being non-terminal (no widget of its own), it is never added to any
//! page's `/Annots`, only to `/Fields`.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId, Stream};
use pdf_document::{
    Command, Document, FieldOrigin, FieldValue, FontFamily, FormField, FormFieldId, FormFieldKind,
    FormFieldSet, PageId,
};
use pdf_form::{FieldAppearance, FrameShape, WidgetFrame};
use pdf_manip::pdf_text_string_object;

use crate::annotations::ObjectSink;
use crate::error::SaveError;

// `/Ff` bit numbers, 1-indexed per ISO 32000-1 Tables 227/229/230 — mirrors
// `pdf_form::read`'s own constants (private to that module, so restated
// here rather than exposed as a cross-crate dependency for four numbers).
const FF_TX_MULTILINE: u32 = 13;
const FF_BTN_RADIO: u32 = 16;
const FF_CH_COMBO: u32 = 18;
const FF_CH_EDIT: u32 = 19;

fn flag_bit(bit: u32) -> i64 {
    1 << (bit - 1)
}

fn rect_array(rect: &pdf_document::Rect) -> Vec<Object> {
    vec![
        Object::Real(rect.x as f32),
        Object::Real(rect.y as f32),
        Object::Real((rect.x + rect.width) as f32),
        Object::Real((rect.y + rect.height) as f32),
    ]
}

fn ft_name(kind: &FormFieldKind) -> &'static str {
    match kind {
        FormFieldKind::Text { .. } => "Tx",
        FormFieldKind::Checkbox | FormFieldKind::RadioGroup { .. } => "Btn",
        FormFieldKind::Dropdown { .. } => "Ch",
        _ => "Tx",
    }
}

fn selected_name_object(selected: &Option<String>) -> Object {
    match selected {
        Some(value) => Object::Name(value.clone().into_bytes()),
        None => Object::Name(b"Off".to_vec()),
    }
}

/// Appends `entry` to `dict_id`'s array-valued `key`, creating the array if
/// absent — used for both a page's `/Annots` and `/AcroForm /Fields`.
fn append_to_array<S: ObjectSink>(
    sink: &mut S,
    dict_id: ObjectId,
    key: &str,
    entry: Object,
) -> Result<(), SaveError> {
    let dict = sink.page_dict_mut(dict_id)?;
    let mut array: Vec<Object> = dict
        .get(key.as_bytes())
        .ok()
        .and_then(|object| object.as_array().ok())
        .cloned()
        .unwrap_or_default();
    array.push(entry);
    dict.set(key, array);
    Ok(())
}

fn standard_font_resources() -> Dictionary {
    let mut fonts = Dictionary::new();
    for family in [
        FontFamily::Helvetica,
        FontFamily::TimesRoman,
        FontFamily::Courier,
    ] {
        let mut font = Dictionary::new();
        font.set("Type", "Font");
        font.set("Subtype", "Type1");
        font.set("BaseFont", pdf_form::base_font_name(family));
        font.set("Encoding", "WinAnsiEncoding");
        fonts.set(pdf_form::resource_name(family), Object::Dictionary(font));
    }
    let mut zapf_dingbats = Dictionary::new();
    zapf_dingbats.set("Type", "Font");
    zapf_dingbats.set("Subtype", "Type1");
    zapf_dingbats.set("BaseFont", "ZapfDingbats");
    fonts.set(
        pdf_form::ZAPF_DINGBATS_RESOURCE,
        Object::Dictionary(zapf_dingbats),
    );

    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    resources
}

/// Gets `catalog_id`'s `/AcroForm`, creating one with a standard-14 `/DR`
/// and an empty `/Fields` if absent. **Public** (per T-138's ficha): the
/// `/Sig` field wiring `pdf-sign`'s `SignatureFieldBuilder` needs (T-073)
/// shares this same `/AcroForm /Fields`/`/DR` — it builds the signature
/// dictionaries but delegates their placement into `/AcroForm`/`/Annots` to
/// the save layer, exactly as this function does for every other field kind.
pub fn ensure_acroform<S: ObjectSink>(
    sink: &mut S,
    catalog_id: ObjectId,
) -> Result<ObjectId, SaveError> {
    {
        let catalog = sink.page_dict_mut(catalog_id)?;
        if let Ok(id) = catalog
            .get(b"AcroForm")
            .and_then(|object| object.as_reference())
        {
            return Ok(id);
        }
    }

    let mut acroform = Dictionary::new();
    acroform.set("Fields", Vec::<Object>::new());
    acroform.set("DR", Object::Dictionary(standard_font_resources()));
    // Decision 2: `/AP` is always generated ourselves, never delegated to a
    // viewer's own appearance-regeneration (which Preview ignores anyway).
    acroform.set("NeedAppearances", false);
    let acroform_id = sink.add_object(Object::Dictionary(acroform));

    let catalog = sink.page_dict_mut(catalog_id)?;
    catalog.set("AcroForm", Object::Reference(acroform_id));
    Ok(acroform_id)
}

/// The pieces of a non-radio field's dictionary that come from its
/// appearance: `/AP`, `/V`, and (checkbox only) `/AS` — computed once and
/// shared between the "build a brand new dict" and "patch an existing one"
/// paths so they can never compute the value differently.
struct SingleFieldAppearance {
    ap: Object,
    value: Object,
    as_state: Option<Object>,
}

fn build_single_field_appearance<S: ObjectSink>(
    sink: &mut S,
    field: &FormField,
    frame: Option<&WidgetFrame>,
) -> Result<SingleFieldAppearance, SaveError> {
    let framed = |mut stream: Stream| {
        if let Some(frame) = frame {
            frame.paint_under(&mut stream, FrameShape::Box);
        }
        stream
    };
    match pdf_form::build_field_appearance(field)? {
        FieldAppearance::Single(stream) => {
            let stream = framed(stream);
            let stream_id = sink.add_object(Object::Stream(stream));
            let mut ap = Dictionary::new();
            ap.set("N", Object::Reference(stream_id));
            let text = match &field.value {
                FieldValue::Text(text) => text.clone(),
                FieldValue::Choice(Some(chosen)) => chosen.clone(),
                FieldValue::Choice(None) => String::new(),
                FieldValue::Checked(_) => String::new(),
            };
            Ok(SingleFieldAppearance {
                ap: Object::Dictionary(ap),
                value: pdf_text_string_object(&text),
                as_state: None,
            })
        }
        FieldAppearance::Checkbox { on_state, on, off } => {
            let (on, off) = (framed(on), framed(off));
            let on_id = sink.add_object(Object::Stream(on));
            let off_id = sink.add_object(Object::Stream(off));
            let mut normal = Dictionary::new();
            normal.set(on_state, Object::Reference(on_id));
            normal.set("Off", Object::Reference(off_id));
            let mut ap = Dictionary::new();
            ap.set("N", Object::Dictionary(normal));
            let checked = matches!(field.value, FieldValue::Checked(true));
            let name = if checked {
                on_state.as_bytes().to_vec()
            } else {
                b"Off".to_vec()
            };
            Ok(SingleFieldAppearance {
                ap: Object::Dictionary(ap),
                value: Object::Name(name.clone()),
                as_state: Some(Object::Name(name)),
            })
        }
        FieldAppearance::Radio(_) => Err(SaveError::InvalidSaveRequest(
            "build_single_field_appearance called on a RadioGroup field",
        )),
    }
}

/// The `/DA` a field of this kind carries — the writer's single per-kind
/// rule, so the "new field" and "patch an existing one" paths can never
/// disagree about it.
///
/// Variable text (`Tx`, `Ch`) gets the whole `TextStyle`; a `/Btn` gets the
/// ZapfDingbats shape, which carries its colour and nothing else (T-205).
/// Every kind gets one: a button's colour is baked into its `/AP` by
/// `pdf_form::build_field_appearance`, and leaving `/DA` off is what used to
/// make that colour unreadable on the way back in.
fn field_da(field: &FormField) -> String {
    match field.kind {
        FormFieldKind::Text { .. } | FormFieldKind::Dropdown { .. } => {
            pdf_form::format_da(&field.style)
        }
        FormFieldKind::Checkbox | FormFieldKind::RadioGroup { .. } => {
            pdf_form::format_button_da(field.style.color)
        }
        // `FormFieldKind` is `#[non_exhaustive]`. A kind added later gets the
        // variable-text shape — the one the spec actually defines `/DA` for,
        // and the one this crate wrote everywhere before T-205 — until
        // whoever adds it decides it needs its own.
        _ => pdf_form::format_da(&field.style),
    }
}

fn write_new_single_field<S: ObjectSink>(
    sink: &mut S,
    acroform_id: ObjectId,
    page_object_id: ObjectId,
    field: &FormField,
) -> Result<(), SaveError> {
    let built = build_single_field_appearance(sink, field, None)?;

    let mut dict = Dictionary::new();
    dict.set("Type", "Annot");
    dict.set("Subtype", "Widget");
    dict.set("FT", ft_name(&field.kind));
    dict.set("T", pdf_text_string_object(&field.name));
    dict.set("Rect", rect_array(&field.rect));
    dict.set("DA", Object::string_literal(field_da(field)));

    match &field.kind {
        FormFieldKind::Text { multiline, max_len } => {
            if *multiline {
                dict.set("Ff", flag_bit(FF_TX_MULTILINE));
            }
            if let Some(max_len) = max_len {
                dict.set("MaxLen", *max_len as i64);
            }
        }
        FormFieldKind::Dropdown { options, editable } => {
            let mut flags = flag_bit(FF_CH_COMBO);
            if *editable {
                flags |= flag_bit(FF_CH_EDIT);
            }
            dict.set("Ff", flags);
            dict.set(
                "Opt",
                options
                    .iter()
                    .map(|option| pdf_text_string_object(option))
                    .collect::<Vec<_>>(),
            );
        }
        _ => {}
    }

    dict.set("V", built.value);
    dict.set("AP", built.ap);
    if let Some(as_state) = built.as_state {
        dict.set("AS", as_state);
    }

    let dict_id = sink.add_object(Object::Dictionary(dict));
    append_to_array(sink, page_object_id, "Annots", Object::Reference(dict_id))?;
    append_to_array(sink, acroform_id, "Fields", Object::Reference(dict_id))?;
    Ok(())
}

fn update_existing_single_field<S: ObjectSink>(
    sink: &mut S,
    field: &FormField,
    object_id: ObjectId,
) -> Result<(), SaveError> {
    let frame = existing_frame(sink, object_id);
    let built = build_single_field_appearance(sink, field, frame.as_ref())?;
    let da = field_da(field);

    let dict = sink.page_dict_mut(object_id)?;
    dict.set("Rect", rect_array(&field.rect));
    dict.set("DA", Object::string_literal(da));
    dict.set("V", built.value);
    dict.set("AP", built.ap);
    if let Some(as_state) = built.as_state {
        dict.set("AS", as_state);
    }
    Ok(())
}

fn selected_choice(field: &FormField) -> Option<String> {
    match &field.value {
        FieldValue::Choice(selected) => selected.clone(),
        _ => None,
    }
}

fn write_new_radio_group<S: ObjectSink>(
    sink: &mut S,
    acroform_id: ObjectId,
    page_object_id: ObjectId,
    field: &FormField,
    options: &[pdf_document::RadioOption],
) -> Result<(), SaveError> {
    let FieldAppearance::Radio(buttons) = pdf_form::build_field_appearance(field)? else {
        return Err(SaveError::InvalidSaveRequest(
            "write_new_radio_group: appearance did not match RadioGroup",
        ));
    };
    let selected = selected_choice(field);

    // Reserved up front so kid widgets can carry a real `/Parent` back-ref —
    // mirrors `write_annotation_object`'s TextNote markup/popup pairing.
    let parent_id = sink.add_object(Object::Dictionary(Dictionary::new()));

    let mut kid_ids = Vec::with_capacity(options.len());
    for (option, button) in options.iter().zip(buttons.iter()) {
        let on_id = sink.add_object(Object::Stream(button.on.clone()));
        let off_id = sink.add_object(Object::Stream(button.off.clone()));
        let mut normal = Dictionary::new();
        normal.set(option.export_value.clone(), Object::Reference(on_id));
        normal.set("Off", Object::Reference(off_id));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Dictionary(normal));

        let is_on = selected.as_deref() == Some(option.export_value.as_str());
        let mut kid = Dictionary::new();
        kid.set("Type", "Annot");
        kid.set("Subtype", "Widget");
        kid.set("Parent", Object::Reference(parent_id));
        kid.set("Rect", rect_array(&option.rect));
        kid.set("AP", ap);
        kid.set(
            "AS",
            if is_on {
                option.export_value.clone()
            } else {
                "Off".to_string()
            },
        );

        let kid_id = sink.add_object(Object::Dictionary(kid));
        append_to_array(sink, page_object_id, "Annots", Object::Reference(kid_id))?;
        kid_ids.push(kid_id);
    }

    let mut parent = Dictionary::new();
    parent.set("FT", "Btn");
    parent.set("T", pdf_text_string_object(&field.name));
    parent.set("Ff", flag_bit(FF_BTN_RADIO));
    // On the parent, next to `/FT` and `/T`: a kid is a widget, not a field,
    // and `/DA` is inheritable (ISO 32000-1 table 220).
    parent.set("DA", Object::string_literal(field_da(field)));
    parent.set("V", selected_name_object(&selected));
    parent.set(
        "Kids",
        kid_ids
            .into_iter()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    sink.set_object(parent_id, Object::Dictionary(parent));
    append_to_array(sink, acroform_id, "Fields", Object::Reference(parent_id))
}

/// Updates an existing radio group's selection and every kid's geometry/
/// appearance. `options[i]` is assumed to correspond to the *i*-th entry of
/// the parent's own `/Kids` — a load-bearing invariant `pdf_form::read`
/// establishes (it builds `options` by walking `/Kids` in array order) and
/// nothing in this crate's `Command` set ever reorders or resizes that list,
/// so the correspondence holds for the whole session.
fn update_existing_radio_group<S: ObjectSink>(
    sink: &mut S,
    field: &FormField,
    parent_id: ObjectId,
    options: &[pdf_document::RadioOption],
) -> Result<(), SaveError> {
    let selected = selected_choice(field);
    let da = field_da(field);
    let kid_ids: Vec<ObjectId> = {
        let parent = sink.page_dict_mut(parent_id)?;
        parent.set("DA", Object::string_literal(da));
        parent.set("V", selected_name_object(&selected));
        parent
            .get(b"Kids")
            .ok()
            .and_then(|object| object.as_array().ok())
            .map(|array| {
                array
                    .iter()
                    .filter_map(|object| object.as_reference().ok())
                    .collect()
            })
            .unwrap_or_default()
    };

    let FieldAppearance::Radio(buttons) = pdf_form::build_field_appearance(field)? else {
        return Err(SaveError::InvalidSaveRequest(
            "update_existing_radio_group: appearance did not match RadioGroup",
        ));
    };
    if kid_ids.len() != options.len() || buttons.len() != options.len() {
        return Err(SaveError::InvalidSaveRequest(
            "existing radio group's /Kids count no longer matches its modeled options",
        ));
    }

    for ((option, button), kid_id) in options.iter().zip(buttons.iter()).zip(kid_ids.iter()) {
        let (mut on, mut off) = (button.on.clone(), button.off.clone());
        if let Some(frame) = existing_frame(sink, *kid_id) {
            frame.paint_under(&mut on, FrameShape::Circle);
            frame.paint_under(&mut off, FrameShape::Circle);
        }
        let on_id = sink.add_object(Object::Stream(on));
        let off_id = sink.add_object(Object::Stream(off));
        let is_on = selected.as_deref() == Some(option.export_value.as_str());

        let kid = sink.page_dict_mut(*kid_id)?;
        kid.set("Rect", rect_array(&option.rect));
        let mut normal = Dictionary::new();
        normal.set(option.export_value.clone(), Object::Reference(on_id));
        normal.set("Off", Object::Reference(off_id));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Dictionary(normal));
        kid.set("AP", ap);
        kid.set(
            "AS",
            if is_on {
                option.export_value.clone()
            } else {
                "Off".to_string()
            },
        );
    }
    Ok(())
}

/// The object ids of the fields this file came with that the session has
/// deleted, in the order they were deleted.
///
/// Read from the applied `RemoveFormField` commands, **not** from a diff of
/// the file's fields against `document.form_fields`: the model only holds
/// what `pdf_form::read_form_fields` can read, so a diff would also "delete"
/// every field it skips — a signature, an unmodelled kind. An undone delete
/// is no longer among the applied entries, and a field that is back in the
/// set (redo after undo cannot do that, but the guard is cheap) is kept.
pub fn removed_existing_fields(document: &Document) -> Vec<ObjectId> {
    let mut removed = Vec::new();
    for command in document.pending_edits.entries() {
        let Command::RemoveFormField(field) = command else {
            continue;
        };
        let FieldOrigin::Existing(id) = field.origin else {
            continue;
        };
        let still_there = document
            .form_fields
            .iter()
            .any(|kept| kept.origin == field.origin);
        if !still_there && !removed.contains(&id) {
            removed.push(id);
        }
    }
    removed
}

/// The fields a save has to write: every `New` one, and only those
/// `Existing` ones an applied command edited.
///
/// An untouched field already has the file's own `/AP`, and ours is not a
/// substitute for it: `pdf_form::build_field_appearance` does not paint the
/// `/MK` border and background a foreign form draws. Regenerating every field
/// on every save stripped each box off the page the first time anything
/// refreshed the preview — and kept doing it after undoing back to zero
/// edits, because the field set itself never empties. Read from the applied
/// commands, the same way [`removed_existing_fields`] is, so an undone edit
/// leaves its field alone again.
pub fn fields_to_write(document: &Document) -> FormFieldSet {
    let edited: HashSet<FormFieldId> = document
        .pending_edits
        .entries()
        .iter()
        .filter_map(|command| match command {
            Command::MoveFormField { id, .. }
            | Command::ResizeFormField { id, .. }
            | Command::RestyleFormField { id, .. }
            | Command::SetFieldValue { id, .. }
            | Command::RenameFormField { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    let mut fields = FormFieldSet::new();
    for field in document.form_fields.iter() {
        if field.origin == FieldOrigin::New || edited.contains(&field.id) {
            fields.insert(field.clone());
        }
    }
    fields
}

/// Takes each of `removed` (see [`removed_existing_fields`]) out of the file:
/// off every page's `/Annots` its widgets sit on, out of `/AcroForm /Fields`,
/// and out of its parent's `/Kids` — pruning a parent that this leaves
/// childless the same way, so no empty naming node outlives its last field.
///
/// Forgetting a field in the model is not enough on its own: the
/// [`write_form_fields`] pass only visits the fields that remain, so a
/// deleted one would come back on reopen. The objects themselves are left
/// unreferenced rather than freed — the incremental writer cannot free an
/// object of a prior revision anyway.
///
/// Looks before it touches: an array is only rewritten when it actually
/// holds the reference, so an incremental save does not carry every page
/// into the new revision just to find the one widget it removes.
pub fn remove_form_fields<S: ObjectSink>(
    sink: &mut S,
    catalog_id: ObjectId,
    page_object_ids: &HashMap<PageId, ObjectId>,
    removed: &[ObjectId],
) -> Result<(), SaveError> {
    if removed.is_empty() {
        return Ok(());
    }
    let mut pages: Vec<ObjectId> = page_object_ids.values().copied().collect();
    pages.sort_unstable();
    for &field_id in removed {
        for widget in widgets_of(sink, field_id) {
            for &page in &pages {
                remove_reference(sink, page, "Annots", widget)?;
            }
        }
        unhook_field(sink, catalog_id, field_id)?;
    }
    Ok(())
}

/// The widget annotations that draw `field_id`: its `/Kids` when it has any
/// (a radio group, or a field with several widgets), itself otherwise (a
/// field merged with its one widget). A modelled field's kids are always
/// widgets — `pdf_form::read` walks a node with named kids as a naming
/// group instead of reading it as a field.
fn widgets_of<S: ObjectSink>(sink: &S, field_id: ObjectId) -> Vec<ObjectId> {
    let kids = dict(sink, field_id)
        .and_then(|field| field.get(b"Kids").ok())
        .and_then(|kids| array(sink, kids))
        .map(|kids| references(&kids))
        .unwrap_or_default();
    if kids.is_empty() {
        vec![field_id]
    } else {
        kids
    }
}

/// Removes `field_id` from wherever it hangs in the field tree: its
/// parent's `/Kids` when it has a parent, `/AcroForm /Fields` otherwise. A
/// parent left with no kids is removed in turn.
fn unhook_field<S: ObjectSink>(
    sink: &mut S,
    catalog_id: ObjectId,
    field_id: ObjectId,
) -> Result<(), SaveError> {
    let parent = dict(sink, field_id)
        .and_then(|field| field.get(b"Parent").ok())
        .and_then(|parent| parent.as_reference().ok());
    // Also tried against `/Fields` even with a parent: a malformed file can
    // list a kid there too, and a dangling entry is what this exists to stop.
    remove_from_fields(sink, catalog_id, field_id)?;
    let Some(parent_id) = parent else {
        return Ok(());
    };
    remove_reference(sink, parent_id, "Kids", field_id)?;
    let childless = dict(sink, parent_id)
        .and_then(|parent| parent.get(b"Kids").ok())
        .and_then(|kids| array(sink, kids))
        .is_none_or(|kids| kids.is_empty());
    if childless {
        unhook_field(sink, catalog_id, parent_id)?;
    }
    Ok(())
}

/// `/AcroForm /Fields`, whether `/AcroForm` is its own object (what
/// [`ensure_acroform`] writes) or a dictionary inline in the catalog (what
/// some producers write).
fn remove_from_fields<S: ObjectSink>(
    sink: &mut S,
    catalog_id: ObjectId,
    field_id: ObjectId,
) -> Result<(), SaveError> {
    let Some(acroform) = dict(sink, catalog_id).and_then(|catalog| catalog.get(b"AcroForm").ok())
    else {
        return Ok(());
    };
    if let Ok(acroform_id) = acroform.as_reference() {
        return remove_reference(sink, acroform_id, "Fields", field_id);
    }
    let Some(fields) = acroform
        .as_dict()
        .ok()
        .and_then(|acroform| acroform.get(b"Fields").ok())
        .and_then(|fields| array(sink, fields))
    else {
        return Ok(());
    };
    let Some(kept) = without(&fields, field_id) else {
        return Ok(());
    };
    // An inline `/AcroForm` whose `/Fields` is itself indirect: rewrite that
    // array object and leave the catalog alone.
    if let Some(Ok(fields_id)) = dict(sink, catalog_id)
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|acroform| acroform.as_dict().ok())
        .and_then(|acroform| acroform.get(b"Fields").ok())
        .map(Object::as_reference)
    {
        sink.set_object(fields_id, Object::Array(kept));
        return Ok(());
    }
    if let Ok(acroform) = sink
        .page_dict_mut(catalog_id)?
        .get_mut(b"AcroForm")
        .and_then(Object::as_dict_mut)
    {
        acroform.set("Fields", kept);
    }
    Ok(())
}

/// Drops `target` from `holder_id`'s array-valued `key` — a direct array or
/// a reference to one. A no-op, touching nothing, when it is not there.
fn remove_reference<S: ObjectSink>(
    sink: &mut S,
    holder_id: ObjectId,
    key: &str,
    target: ObjectId,
) -> Result<(), SaveError> {
    let Some(value) = dict(sink, holder_id).and_then(|holder| holder.get(key.as_bytes()).ok())
    else {
        return Ok(());
    };
    let indirect = value.as_reference().ok();
    let Some(kept) = array(sink, value).and_then(|entries| without(&entries, target)) else {
        return Ok(());
    };
    match indirect {
        Some(array_id) => sink.set_object(array_id, Object::Array(kept)),
        None => sink.page_dict_mut(holder_id)?.set(key, kept),
    }
    Ok(())
}

/// `entries` without any reference to `target`, or `None` when there was
/// none to take out — the caller's signal not to write anything.
fn without(entries: &[Object], target: ObjectId) -> Option<Vec<Object>> {
    let kept: Vec<Object> = entries
        .iter()
        .filter(|entry| entry.as_reference().ok() != Some(target))
        .cloned()
        .collect();
    (kept.len() != entries.len()).then_some(kept)
}

/// The `/MK` frame the widget at `id` already draws, so a regenerated `/AP`
/// keeps it — see [`WidgetFrame`].
fn existing_frame<S: ObjectSink>(sink: &S, id: ObjectId) -> Option<WidgetFrame> {
    WidgetFrame::from_widget(dict(sink, id)?, |object| match object {
        Object::Reference(id) => sink.object(*id),
        direct => Some(direct),
    })
}

fn dict<S: ObjectSink>(sink: &S, id: ObjectId) -> Option<&Dictionary> {
    sink.object(id).and_then(|object| object.as_dict().ok())
}

/// An array value, following one reference if the value is indirect.
fn array<S: ObjectSink>(sink: &S, value: &Object) -> Option<Vec<Object>> {
    match value {
        Object::Reference(id) => sink.object(*id)?.as_array().ok().cloned(),
        other => other.as_array().ok().cloned(),
    }
}

fn references(entries: &[Object]) -> Vec<ObjectId> {
    entries
        .iter()
        .filter_map(|entry| entry.as_reference().ok())
        .collect()
}

/// Writes every field in `form_fields` into `sink`: a `New` field gets a
/// fresh object (or, for a `RadioGroup`, a parent plus one kid per option)
/// appended to both its page's `/Annots` and `/AcroForm /Fields`; an
/// `Existing` field is patched in place at its original object id and
/// touches neither array (see the module docs for why).
pub fn write_form_fields<S: ObjectSink>(
    sink: &mut S,
    catalog_id: ObjectId,
    page_object_ids: &HashMap<PageId, ObjectId>,
    form_fields: &FormFieldSet,
) -> Result<(), SaveError> {
    if form_fields.is_empty() {
        return Ok(());
    }
    let acroform_id = ensure_acroform(sink, catalog_id)?;

    for field in form_fields.iter() {
        match field.origin {
            FieldOrigin::New => {
                let page_object_id =
                    *page_object_ids
                        .get(&field.page)
                        .ok_or(SaveError::InvalidSaveRequest(
                            "form field references a page id not present in the saved document",
                        ))?;
                match &field.kind {
                    FormFieldKind::RadioGroup { options } => {
                        write_new_radio_group(sink, acroform_id, page_object_id, field, options)?
                    }
                    _ => write_new_single_field(sink, acroform_id, page_object_id, field)?,
                }
            }
            FieldOrigin::Existing(object_id) => match &field.kind {
                FormFieldKind::RadioGroup { options } => {
                    update_existing_radio_group(sink, field, object_id, options)?
                }
                _ => update_existing_single_field(sink, field, object_id)?,
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{
        Color, FieldOrigin, FontFamily, FormFieldId, PageId, RadioOption, Rect, TextStyle,
    };

    fn style() -> TextStyle {
        TextStyle {
            font: FontFamily::Helvetica,
            size_pt: 12.0,
            color: Color { r: 0, g: 0, b: 0 },
        }
    }

    /// A style whose family and size a button ignores and whose colour it
    /// does not — the shape T-205's `/DA` has to carry.
    fn red_style() -> TextStyle {
        TextStyle {
            font: FontFamily::TimesRoman,
            size_pt: 10.0,
            color: Color { r: 255, g: 0, b: 0 },
        }
    }

    /// What [`red_style`]'s colour serializes to on a `/Btn`, written out
    /// literally so the assertions pin the bytes rather than re-deriving
    /// them from the same function under test.
    const RED_BUTTON_DA: &[u8] = b"/ZaDb 0 Tf 1 0 0 rg";

    fn one_page_doc() -> lopdf::Document {
        use lopdf::dictionary;
        let mut doc = lopdf::Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn text_field(rect: Rect) -> FormField {
        FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Name".to_string(),
            rect,
            style: style(),
            value: FieldValue::Text("Ada".to_string()),
            kind: FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: FieldOrigin::New,
        }
    }

    fn rect() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 20.0,
        }
    }

    #[test]
    fn ensure_acroform_creates_one_with_standard_fonts() {
        let mut doc = one_page_doc();
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let acroform_id = ensure_acroform(&mut doc, catalog_id).expect("should create AcroForm");
        let acroform = doc.get_dictionary(acroform_id).unwrap();
        let dr = acroform.get(b"DR").unwrap().as_dict().unwrap();
        let fonts = dr.get(b"Font").unwrap().as_dict().unwrap();
        assert!(fonts.has(b"Helv"));
        assert!(fonts.has(b"TiRo"));
        assert!(fonts.has(b"Cour"));
        assert!(fonts.has(b"ZaDb"));
        assert!(acroform
            .get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn ensure_acroform_reuses_an_existing_one() {
        let mut doc = one_page_doc();
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let first = ensure_acroform(&mut doc, catalog_id).unwrap();
        let second = ensure_acroform(&mut doc, catalog_id).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn writes_a_new_text_field_into_fields_and_annots() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let mut set = FormFieldSet::new();
        set.insert(text_field(rect()));

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        let acroform_id = doc
            .get_dictionary(catalog_id)
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_reference()
            .unwrap();
        let fields = doc
            .get_dictionary(acroform_id)
            .unwrap()
            .get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(fields.len(), 1);
        let field_id = fields[0].as_reference().unwrap();

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(annots[0].as_reference().unwrap(), field_id);

        let dict = doc.get_dictionary(field_id).unwrap();
        assert_eq!(dict.get(b"FT").unwrap().as_name().unwrap(), b"Tx");
        assert_eq!(dict.get(b"T").unwrap().as_str().unwrap(), b"Name");
        match dict.get(b"V").unwrap() {
            Object::String(bytes, _) => assert_eq!(bytes, b"Ada"),
            other => panic!("expected string V, got {other:?}"),
        }
        assert!(dict.has(b"AP"));
        assert!(dict.has(b"DA"));
    }

    #[test]
    fn accented_name_value_and_options_round_trip() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let mut set = FormFieldSet::new();
        set.insert(FormField {
            name: "Teléfono".to_string(),
            value: FieldValue::Text("José Ñandú".to_string()),
            ..text_field(rect())
        });
        set.insert(FormField {
            id: FormFieldId(2),
            name: "Opción".to_string(),
            value: FieldValue::Choice(Some("Sí".to_string())),
            kind: FormFieldKind::Dropdown {
                options: vec!["Sí".to_string(), "No".to_string()],
                editable: false,
            },
            ..text_field(rect())
        });

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        // Written as UTF-8, "é" used to read back as "Ã©".
        let read = pdf_form::read_form_fields(&doc);
        let text = read
            .iter()
            .find(|f| f.name == "Teléfono")
            .expect("text field");
        assert_eq!(text.value, FieldValue::Text("José Ñandú".to_string()));
        let dropdown = read.iter().find(|f| f.name == "Opción").expect("dropdown");
        assert_eq!(dropdown.value, FieldValue::Choice(Some("Sí".to_string())));
        match &dropdown.kind {
            FormFieldKind::Dropdown { options, .. } => {
                assert_eq!(options, &vec!["Sí".to_string(), "No".to_string()])
            }
            other => panic!("expected Dropdown, got {other:?}"),
        }
    }

    #[test]
    fn writes_a_new_checkbox_with_yes_off_states() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let mut set = FormFieldSet::new();
        set.insert(FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Agree".to_string(),
            rect: rect(),
            style: style(),
            value: FieldValue::Checked(true),
            kind: FormFieldKind::Checkbox,
            origin: FieldOrigin::New,
        });

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        let dict = doc
            .get_dictionary(annots[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(dict.get(b"V").unwrap().as_name().unwrap(), b"Yes");
        assert_eq!(dict.get(b"AS").unwrap().as_name().unwrap(), b"Yes");
        let ap_n = dict
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(ap_n.has(b"Yes"));
        assert!(ap_n.has(b"Off"));
    }

    #[test]
    fn writes_a_new_radio_group_with_one_kid_per_option() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let mut set = FormFieldSet::new();
        set.insert(FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Choice".to_string(),
            rect: rect(),
            style: style(),
            value: FieldValue::Choice(Some("Yes".to_string())),
            kind: FormFieldKind::RadioGroup {
                options: vec![
                    RadioOption {
                        export_value: "Yes".to_string(),
                        rect: rect(),
                    },
                    RadioOption {
                        export_value: "No".to_string(),
                        rect: Rect { x: 40.0, ..rect() },
                    },
                ],
            },
            origin: FieldOrigin::New,
        });

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            annots.len(),
            2,
            "one Annots entry per radio kid, none for the parent"
        );

        let acroform_id = doc
            .get_dictionary(catalog_id)
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_reference()
            .unwrap();
        let fields = doc
            .get_dictionary(acroform_id)
            .unwrap()
            .get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(fields.len(), 1, "exactly one Fields entry: the parent");
        let parent = doc
            .get_dictionary(fields[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(parent.get(b"V").unwrap().as_name().unwrap(), b"Yes");
        let kids = parent.get(b"Kids").unwrap().as_array().unwrap();
        assert_eq!(kids.len(), 2);

        let yes_kid = doc.get_dictionary(kids[0].as_reference().unwrap()).unwrap();
        assert_eq!(yes_kid.get(b"AS").unwrap().as_name().unwrap(), b"Yes");
        assert_eq!(
            yes_kid.get(b"Parent").unwrap().as_reference().unwrap(),
            fields[0].as_reference().unwrap()
        );
        let no_kid = doc.get_dictionary(kids[1].as_reference().unwrap()).unwrap();
        assert_eq!(no_kid.get(b"AS").unwrap().as_name().unwrap(), b"Off");
    }

    /// T-205: a `/Btn` carries a `/DA` too, in the ZapfDingbats shape — the
    /// resource its own appearance draws from, at the viewer-chosen size,
    /// with the colour that appearance is painted in.
    #[test]
    fn a_new_checkbox_carries_the_zapf_dingbats_da() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let mut set = FormFieldSet::new();
        set.insert(FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Agree".to_string(),
            rect: rect(),
            style: red_style(),
            value: FieldValue::Checked(true),
            kind: FormFieldKind::Checkbox,
            origin: FieldOrigin::New,
        });

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        let dict = doc
            .get_dictionary(annots[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(dict.get(b"DA").unwrap().as_str().unwrap(), RED_BUTTON_DA);
    }

    /// The radio group's `/DA` belongs on the parent field dict, next to its
    /// `/FT` and `/T` — a kid is a widget, not a field, and inherits it.
    #[test]
    fn a_new_radio_groups_parent_carries_the_zapf_dingbats_da() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();

        let mut set = FormFieldSet::new();
        set.insert(FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Choice".to_string(),
            rect: rect(),
            style: red_style(),
            value: FieldValue::Choice(Some("Yes".to_string())),
            kind: FormFieldKind::RadioGroup {
                options: vec![RadioOption {
                    export_value: "Yes".to_string(),
                    rect: rect(),
                }],
            },
            origin: FieldOrigin::New,
        });

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        let acroform_id = doc
            .get_dictionary(catalog_id)
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_reference()
            .unwrap();
        let fields = doc
            .get_dictionary(acroform_id)
            .unwrap()
            .get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap();
        let parent = doc
            .get_dictionary(fields[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(parent.get(b"DA").unwrap().as_str().unwrap(), RED_BUTTON_DA);

        let kids = parent.get(b"Kids").unwrap().as_array().unwrap();
        let kid = doc.get_dictionary(kids[0].as_reference().unwrap()).unwrap();
        assert!(
            !kid.has(b"DA"),
            "a kid widget inherits the field's /DA rather than restating it"
        );
    }

    /// An existing button is patched the same way an existing text field is
    /// — the writer has one `/DA` rule per kind, not one per code path.
    #[test]
    fn updating_an_existing_checkbox_writes_its_da() {
        use lopdf::dictionary;

        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let field_id = doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("Agree"),
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        doc.get_dictionary_mut(page_object_id)
            .unwrap()
            .set("Annots", vec![Object::Reference(field_id)]);
        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![Object::Reference(field_id)],
        });
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(catalog_id)
            .unwrap()
            .set("AcroForm", acroform_id);

        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let mut set = FormFieldSet::new();
        set.insert(FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Agree".to_string(),
            rect: rect(),
            style: red_style(),
            value: FieldValue::Checked(true),
            kind: FormFieldKind::Checkbox,
            origin: FieldOrigin::Existing(field_id),
        });

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        let dict = doc.get_dictionary(field_id).unwrap();
        assert_eq!(dict.get(b"DA").unwrap().as_str().unwrap(), RED_BUTTON_DA);
    }

    #[test]
    fn updating_an_existing_field_touches_neither_fields_nor_annots() {
        use lopdf::dictionary;

        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let field_id = doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("Name"),
            "V" => Object::string_literal("old"),
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        doc.get_dictionary_mut(page_object_id)
            .unwrap()
            .set("Annots", vec![Object::Reference(field_id)]);
        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![Object::Reference(field_id)],
        });
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(catalog_id)
            .unwrap()
            .set("AcroForm", acroform_id);

        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let mut set = FormFieldSet::new();
        set.insert(FormField {
            id: FormFieldId(1),
            page: PageId(0),
            name: "Name".to_string(),
            rect: rect(),
            style: style(),
            value: FieldValue::Text("new".to_string()),
            kind: FormFieldKind::Text {
                multiline: false,
                max_len: None,
            },
            origin: FieldOrigin::Existing(field_id),
        });

        write_form_fields(&mut doc, catalog_id, &page_ids, &set).expect("should write");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(annots.len(), 1, "no new Annots entry for an existing field");
        assert_eq!(annots[0].as_reference().unwrap(), field_id);

        let fields = doc
            .get_dictionary(acroform_id)
            .unwrap()
            .get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(fields.len(), 1, "no new Fields entry for an existing field");

        let dict = doc.get_dictionary(field_id).unwrap();
        match dict.get(b"V").unwrap() {
            Object::String(bytes, _) => assert_eq!(bytes, b"new"),
            other => panic!("expected string V, got {other:?}"),
        }
        assert_eq!(
            dict.get(b"T").unwrap().as_str().unwrap(),
            b"Name",
            "T is never touched when updating an existing field"
        );
    }

    /// A page with one hierarchical field `address.street`: a naming parent
    /// listed in `/Fields`, and a terminal kid merged with its widget. The
    /// page's `/Annots` and the inline `/AcroForm`'s `/Fields` are both
    /// *indirect* arrays — the two shapes `append_to_array` never meets
    /// because this crate never writes them, and a delete must still handle.
    fn hierarchical_doc() -> (
        lopdf::Document,
        ObjectId,
        ObjectId,
        ObjectId,
        ObjectId,
        ObjectId,
    ) {
        use lopdf::dictionary;

        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let parent_id = doc.new_object_id();
        let street_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("street"),
            "Parent" => parent_id,
            "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        doc.objects.insert(
            parent_id,
            Object::Dictionary(dictionary! {
                "T" => Object::string_literal("address"),
                "Kids" => vec![Object::Reference(street_id)],
            }),
        );
        let other_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("other"),
            "Rect" => vec![0.into(), 20.into(), 10.into(), 30.into()],
        });
        let annots_id = doc.add_object(Object::Array(vec![
            Object::Reference(street_id),
            Object::Reference(other_id),
        ]));
        doc.get_dictionary_mut(page_object_id)
            .unwrap()
            .set("Annots", annots_id);
        let fields_id = doc.add_object(Object::Array(vec![
            Object::Reference(parent_id),
            Object::Reference(other_id),
        ]));
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(catalog_id)
            .unwrap()
            .set("AcroForm", dictionary! { "Fields" => fields_id });
        (
            doc,
            catalog_id,
            page_object_id,
            street_id,
            annots_id,
            fields_id,
        )
    }

    fn references_in(doc: &lopdf::Document, array_id: ObjectId) -> Vec<ObjectId> {
        references(doc.get_object(array_id).unwrap().as_array().unwrap())
    }

    #[test]
    fn removing_a_kid_prunes_its_childless_parent_through_indirect_arrays() {
        let (mut doc, catalog_id, page_object_id, street_id, annots_id, fields_id) =
            hierarchical_doc();
        let parent_id = doc
            .get_dictionary(street_id)
            .unwrap()
            .get(b"Parent")
            .unwrap()
            .as_reference()
            .unwrap();
        let other_id = references_in(&doc, annots_id)[1];
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);

        remove_form_fields(&mut doc, catalog_id, &page_ids, &[street_id]).expect("should remove");

        assert_eq!(
            references_in(&doc, annots_id),
            [other_id],
            "widget off the page"
        );
        assert_eq!(
            references_in(&doc, fields_id),
            [other_id],
            "the childless parent is out of /Fields"
        );
        assert!(
            doc.get_dictionary(parent_id)
                .unwrap()
                .get(b"Kids")
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty(),
            "and the kid is out of its parent"
        );
        assert!(
            doc.get_dictionary(page_object_id)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_reference()
                .is_ok(),
            "an indirect /Annots stays indirect"
        );
    }

    #[test]
    fn removing_nothing_touches_nothing() {
        let (mut doc, catalog_id, page_object_id, _, annots_id, fields_id) = hierarchical_doc();
        let before = doc.objects.clone();
        let page_ids = HashMap::from([(PageId(0), page_object_id)]);

        remove_form_fields(&mut doc, catalog_id, &page_ids, &[]).expect("should no-op");

        assert_eq!(doc.objects, before);
        assert_eq!(references_in(&doc, annots_id).len(), 2);
        assert_eq!(references_in(&doc, fields_id).len(), 2);
    }

    #[test]
    fn an_undone_delete_is_not_a_removed_field() {
        let mut document = Document::default();
        let mut field = text_field(rect());
        field.origin = FieldOrigin::Existing((7, 0));
        document.form_fields.insert(field.clone());
        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.apply(&mut document, Command::RemoveFormField(field)));
        document.pending_edits = log;
        assert_eq!(removed_existing_fields(&document), [(7, 0)]);

        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.undo(&mut document));
        document.pending_edits = log;

        assert!(removed_existing_fields(&document).is_empty());
    }

    #[test]
    fn deleting_a_new_field_removes_nothing_from_the_file() {
        let mut document = Document::default();
        let field = text_field(rect());
        assert_eq!(field.origin, FieldOrigin::New);
        document.form_fields.insert(field.clone());
        let mut log = std::mem::take(&mut document.pending_edits);
        assert!(log.apply(&mut document, Command::RemoveFormField(field)));
        document.pending_edits = log;

        assert!(removed_existing_fields(&document).is_empty());
    }
}
