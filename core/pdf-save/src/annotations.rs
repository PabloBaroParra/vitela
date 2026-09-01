//! Writes `pdf_document::Annotation`s into a real lopdf object graph, and
//! assigns the real indirect object ids `pdf-annotate`'s appearance builders
//! leave as `Object::Reference((0, 0))` placeholders (T-032, per the note on
//! `pdf-annotate::appearance` — "Object numbering ... is `pdf-save`'s job at
//! write time").
//!
//! [`ObjectSink`] abstracts over the two writer targets so this module's
//! logic is shared between the full-rewrite writer (`lopdf::Document`) and
//! the incremental writer (`lopdf::IncrementalDocument`, which must clone a
//! page into its `new_document` before mutating it — see
//! `IncrementalDocument::opt_clone_object_to_new_document`).
//!
//! ## Known gap (documented, not fixed in this batch)
//!
//! Existing `/Annots` are not yet parsed into editable `pdf_document::Annotation`
//! values on open. They are instead carried as opaque PDF objects by
//! [`crate::bridge::page_annotation_objects`] and included whenever this
//! module appends in-session annotations, so unknown or unsupported annotation
//! subtypes are preserved without claiming that the editor can modify them.

use std::collections::HashMap;

use lopdf::{Dictionary, Object, ObjectId, Stream};
use pdf_document::{Annotation, AnnotationKind, AnnotationSet, Color, PageId, Rect};

use crate::error::SaveError;

/// Abstracts "add/replace an object, get a page dict ready to mutate, and
/// read/mutate the trailer" over `lopdf::Document` (full-rewrite) and
/// `lopdf::IncrementalDocument` (incremental — must clone-before-mutate).
///
/// The trailer accessors exist for [`crate::metadata::apply_document_info`]
/// (T-171): resolving and, when absent, creating the `/Info` dictionary
/// needs to read the trailer's `/Info` reference and, on a fresh dict, point
/// the trailer at it — on the incremental side that means
/// `new_document.trailer`, which starts as a full clone of the previous
/// revision's trailer (`Document::new_from_prev`) and is what a save
/// actually writes.
pub trait ObjectSink {
    fn add_object(&mut self, object: Object) -> ObjectId;
    fn set_object(&mut self, id: ObjectId, object: Object);
    fn page_dict_mut(&mut self, page_object_id: ObjectId) -> Result<&mut Dictionary, SaveError>;
    fn trailer(&self) -> &Dictionary;
    fn trailer_mut(&mut self) -> &mut Dictionary;
}

impl ObjectSink for lopdf::Document {
    fn add_object(&mut self, object: Object) -> ObjectId {
        lopdf::Document::add_object(self, object)
    }

    fn set_object(&mut self, id: ObjectId, object: Object) {
        self.objects.insert(id, object);
    }

    fn page_dict_mut(&mut self, page_object_id: ObjectId) -> Result<&mut Dictionary, SaveError> {
        self.get_dictionary_mut(page_object_id).map_err(Into::into)
    }

    fn trailer(&self) -> &Dictionary {
        &self.trailer
    }

    fn trailer_mut(&mut self) -> &mut Dictionary {
        &mut self.trailer
    }
}

impl ObjectSink for lopdf::IncrementalDocument {
    fn add_object(&mut self, object: Object) -> ObjectId {
        self.new_document.add_object(object)
    }

    fn set_object(&mut self, id: ObjectId, object: Object) {
        self.new_document.objects.insert(id, object);
    }

    fn page_dict_mut(&mut self, page_object_id: ObjectId) -> Result<&mut Dictionary, SaveError> {
        self.opt_clone_object_to_new_document(page_object_id)?;
        self.new_document
            .get_object_mut(page_object_id)
            .and_then(Object::as_dict_mut)
            .map_err(Into::into)
    }

    fn trailer(&self) -> &Dictionary {
        &self.new_document.trailer
    }

    fn trailer_mut(&mut self) -> &mut Dictionary {
        &mut self.new_document.trailer
    }
}

fn color_array(color: Color) -> Vec<Object> {
    vec![
        Object::Real(f32::from(color.r) / 255.0),
        Object::Real(f32::from(color.g) / 255.0),
        Object::Real(f32::from(color.b) / 255.0),
    ]
}

fn rect_array(rect: &Rect) -> Vec<Object> {
    vec![
        Object::Real(rect.x as f32),
        Object::Real(rect.y as f32),
        Object::Real((rect.x + rect.width) as f32),
        Object::Real((rect.y + rect.height) as f32),
    ]
}

/// Quad points for a single-rectangle text-markup annotation (Highlight,
/// Underline, StrikeOut): top-left, top-right, bottom-left, bottom-right, per
/// PDF 32000-1:2008 §12.5.6.10. Real text-selection-derived quad points (one
/// quad per selected line) are a future-phase enhancement — MVP approximates
/// with a single quad covering the whole `rect`.
fn quad_points(rect: &Rect) -> Vec<Object> {
    let (x0, y0) = (rect.x, rect.y);
    let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
    vec![
        Object::Real(x0 as f32),
        Object::Real(y1 as f32),
        Object::Real(x1 as f32),
        Object::Real(y1 as f32),
        Object::Real(x0 as f32),
        Object::Real(y0 as f32),
        Object::Real(x1 as f32),
        Object::Real(y0 as f32),
    ]
}

fn text_markup_dict(subtype: &str, rect: &Rect, color: Color) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", "Annot");
    dict.set("Subtype", subtype);
    dict.set("Rect", rect_array(rect));
    dict.set("C", color_array(color));
    dict.set("QuadPoints", quad_points(rect));
    dict
}

fn shape_dict(rect: &Rect, color: Color) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", "Annot");
    dict.set("Subtype", "Square");
    dict.set("Rect", rect_array(rect));
    dict.set("C", color_array(color));
    dict
}

fn ink_dict(points: &[(f64, f64)], color: Color) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", "Annot");
    dict.set("Subtype", "Ink");

    let (min_x, max_x, min_y, max_y) = points.iter().fold(
        (f64::MAX, f64::MIN, f64::MAX, f64::MIN),
        |(mn_x, mx_x, mn_y, mx_y), &(x, y)| (mn_x.min(x), mx_x.max(x), mn_y.min(y), mx_y.max(y)),
    );
    dict.set(
        "Rect",
        vec![
            Object::Real(min_x as f32),
            Object::Real(min_y as f32),
            Object::Real(max_x as f32),
            Object::Real(max_y as f32),
        ],
    );

    let flat: Vec<Object> = points
        .iter()
        .flat_map(|&(x, y)| [Object::Real(x as f32), Object::Real(y as f32)])
        .collect();
    dict.set("InkList", vec![Object::Array(flat)]);
    dict.set("C", color_array(color));
    dict
}

/// Builds the `/AP /N` **Form** XObject wrapping `image_id` (an Image
/// XObject) and the owning Stamp annotation dict. `pdf-annotate`'s
/// `build_stamp_appearance` returns only the raw Image XObject — per the PDF
/// spec, `/AP /N` must reference a Form XObject (with its own `/BBox` and
/// `/Resources`), not an Image XObject directly; wrapping it is this
/// module's job (T-032, per `appearance.rs`'s "pdf-save assigns the real
/// object id" note).
fn stamp_annotation_dict<S: ObjectSink>(
    sink: &mut S,
    rect: &Rect,
    image_id: ObjectId,
) -> Dictionary {
    let (width, height) = (rect.width, rect.height);

    let mut form_dict = Dictionary::new();
    form_dict.set("Type", "XObject");
    form_dict.set("Subtype", "Form");
    form_dict.set(
        "BBox",
        vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(width as f32),
            Object::Real(height as f32),
        ],
    );
    let mut xobjects = Dictionary::new();
    xobjects.set("Im0", Object::Reference(image_id));
    let mut resources = Dictionary::new();
    resources.set("XObject", xobjects);
    form_dict.set("Resources", resources);

    let content = format!("q {width} 0 0 {height} 0 0 cm /Im0 Do Q");
    let form_id = sink.add_object(Object::Stream(Stream::new(form_dict, content.into_bytes())));

    let mut dict = Dictionary::new();
    dict.set("Type", "Annot");
    dict.set("Subtype", "Stamp");
    dict.set("Rect", rect_array(rect));
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(form_id));
    dict.set("AP", ap);
    dict
}

fn write_annotation_object<S: ObjectSink>(
    sink: &mut S,
    annotation: &Annotation,
) -> Result<ObjectId, SaveError> {
    match &annotation.kind {
        AnnotationKind::TextNote { .. } => {
            let (mut markup, mut popup) = pdf_annotate::build_text_note_dicts(annotation)?;
            let markup_id = sink.add_object(Object::Dictionary(Dictionary::new()));
            let popup_id = sink.add_object(Object::Dictionary(Dictionary::new()));
            markup.set("Popup", Object::Reference(popup_id));
            popup.set("Parent", Object::Reference(markup_id));
            sink.set_object(markup_id, Object::Dictionary(markup));
            sink.set_object(popup_id, Object::Dictionary(popup));
            Ok(markup_id)
        }
        AnnotationKind::Stamp { rect, .. } => {
            let appearance = pdf_annotate::build_stamp_appearance(annotation)?;
            let smask_id = appearance
                .smask_xobject
                .map(|smask| sink.add_object(Object::Stream(smask)));
            let mut image_xobject = appearance.image_xobject;
            if let Some(smask_id) = smask_id {
                image_xobject.dict.set("SMask", Object::Reference(smask_id));
            }
            let image_id = sink.add_object(Object::Stream(image_xobject));
            let dict = stamp_annotation_dict(sink, rect, image_id);
            Ok(sink.add_object(Object::Dictionary(dict)))
        }
        AnnotationKind::Highlight { rect, color } => Ok(sink.add_object(Object::Dictionary(
            text_markup_dict("Highlight", rect, *color),
        ))),
        AnnotationKind::Underline { rect, color } => Ok(sink.add_object(Object::Dictionary(
            text_markup_dict("Underline", rect, *color),
        ))),
        AnnotationKind::Strikeout { rect, color } => Ok(sink.add_object(Object::Dictionary(
            text_markup_dict("StrikeOut", rect, *color),
        ))),
        AnnotationKind::Shape { rect, color } => {
            Ok(sink.add_object(Object::Dictionary(shape_dict(rect, *color))))
        }
        AnnotationKind::Ink { points, color } => {
            Ok(sink.add_object(Object::Dictionary(ink_dict(points, *color))))
        }
        _ => Err(SaveError::InvalidSaveRequest(
            "unsupported annotation kind for save (unhandled non_exhaustive variant)",
        )),
    }
}

/// Writes every annotation in `annotations` into `sink`, grouping by page and
/// appending them after `existing_annotations` in each touched page's
/// `/Annots` array.
pub fn attach_annotations<S: ObjectSink>(
    sink: &mut S,
    page_object_ids: &HashMap<PageId, ObjectId>,
    existing_annotations: &HashMap<PageId, Vec<Object>>,
    annotations: &AnnotationSet,
) -> Result<(), SaveError> {
    let mut by_page: HashMap<PageId, Vec<ObjectId>> = HashMap::new();

    for annotation in annotations.iter() {
        if !page_object_ids.contains_key(&annotation.page) {
            return Err(SaveError::InvalidSaveRequest(
                "annotation references a page id not present in the saved document",
            ));
        }
        let annotation_ref = write_annotation_object(sink, annotation)?;
        by_page
            .entry(annotation.page)
            .or_default()
            .push(annotation_ref);
    }

    for (page_id, refs) in by_page {
        let page_object_id = page_object_ids[&page_id];
        let mut entries = existing_annotations
            .get(&page_id)
            .cloned()
            .unwrap_or_default();
        entries.extend(refs.into_iter().map(Object::Reference));
        let dict = sink.page_dict_mut(page_object_id)?;
        dict.set("Annots", entries);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{AnnotationId, Popup};

    fn rect() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
        }
    }

    fn color() -> Color {
        Color {
            r: 200,
            g: 10,
            b: 10,
        }
    }

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

    #[test]
    fn attach_highlight_sets_annots_on_the_target_page() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let mut page_ids = HashMap::new();
        page_ids.insert(PageId(0), page_object_id);

        let mut set = AnnotationSet::new();
        set.insert(Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: rect(),
                color: color(),
            },
        });

        attach_annotations(&mut doc, &page_ids, &HashMap::new(), &set)
            .expect("attach should succeed");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .and_then(|o| o.as_array())
            .unwrap();
        assert_eq!(annots.len(), 1);

        let annot_ref = annots[0].as_reference().unwrap();
        let annot_dict = doc.get_dictionary(annot_ref).unwrap();
        assert_eq!(
            annot_dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Highlight"
        );
        assert!(annot_dict.has(b"QuadPoints"));
    }

    #[test]
    fn attach_text_note_links_popup_and_parent_with_real_ids() {
        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let mut page_ids = HashMap::new();
        page_ids.insert(PageId(0), page_object_id);

        let mut set = AnnotationSet::new();
        set.insert(Annotation {
            id: AnnotationId(2),
            page: PageId(0),
            kind: AnnotationKind::TextNote {
                rect: rect(),
                contents: "hi".to_string(),
                popup: Popup {
                    is_open: false,
                    contents: "hi".to_string(),
                },
            },
        });

        attach_annotations(&mut doc, &page_ids, &HashMap::new(), &set)
            .expect("attach should succeed");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .and_then(|o| o.as_array())
            .unwrap();
        let markup_ref = annots[0].as_reference().unwrap();
        let markup_dict = doc.get_dictionary(markup_ref).unwrap();
        let popup_ref = markup_dict.get(b"Popup").unwrap().as_reference().unwrap();
        assert_ne!(popup_ref, (0, 0));

        let popup_dict = doc.get_dictionary(popup_ref).unwrap();
        let parent_ref = popup_dict.get(b"Parent").unwrap().as_reference().unwrap();
        assert_eq!(parent_ref, markup_ref);
    }

    #[test]
    fn attach_stamp_wraps_image_in_a_form_xobject() {
        use image::{ImageFormat, RgbImage};
        use std::io::Cursor;

        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let mut page_ids = HashMap::new();
        page_ids.insert(PageId(0), page_object_id);

        let dynamic =
            image::DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3])));
        let mut buf = Cursor::new(Vec::new());
        dynamic.write_to(&mut buf, ImageFormat::Png).unwrap();

        let mut set = AnnotationSet::new();
        set.insert(Annotation {
            id: AnnotationId(3),
            page: PageId(0),
            kind: AnnotationKind::Stamp {
                rect: rect(),
                image_bytes: buf.into_inner(),
                has_alpha: false,
            },
        });

        attach_annotations(&mut doc, &page_ids, &HashMap::new(), &set)
            .expect("attach should succeed");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .and_then(|o| o.as_array())
            .unwrap();
        let stamp_ref = annots[0].as_reference().unwrap();
        let stamp_dict = doc.get_dictionary(stamp_ref).unwrap();
        let ap_dict = stamp_dict.get(b"AP").unwrap().as_dict().unwrap();
        let form_ref = ap_dict.get(b"N").unwrap().as_reference().unwrap();
        let form_object = doc.get_object(form_ref).unwrap();
        let form_stream = form_object.as_stream().unwrap();
        assert_eq!(
            form_stream.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Form"
        );
    }

    #[test]
    fn attach_annotations_rejects_unknown_page() {
        let mut doc = one_page_doc();
        let page_ids: HashMap<PageId, ObjectId> = HashMap::new();

        let mut set = AnnotationSet::new();
        set.insert(Annotation {
            id: AnnotationId(4),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: rect(),
                color: color(),
            },
        });

        let result = attach_annotations(&mut doc, &page_ids, &HashMap::new(), &set);
        assert!(matches!(result, Err(SaveError::InvalidSaveRequest(_))));
    }

    #[test]
    fn attach_annotations_preserves_existing_page_entries() {
        use lopdf::dictionary;

        let mut doc = one_page_doc();
        let page_object_id = *doc.get_pages().get(&1).unwrap();
        let existing_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Contents" => "created elsewhere",
        });
        doc.get_dictionary_mut(page_object_id)
            .unwrap()
            .set("Annots", vec![Object::Reference(existing_id)]);

        let page_ids = HashMap::from([(PageId(0), page_object_id)]);
        let existing = HashMap::from([(PageId(0), vec![Object::Reference(existing_id)])]);
        let mut set = AnnotationSet::new();
        set.insert(Annotation {
            id: AnnotationId(5),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: rect(),
                color: color(),
            },
        });

        attach_annotations(&mut doc, &page_ids, &existing, &set).expect("attach should succeed");

        let annots = doc
            .get_dictionary(page_object_id)
            .unwrap()
            .get(b"Annots")
            .and_then(|object| object.as_array())
            .unwrap();
        assert_eq!(annots.len(), 2);
        assert_eq!(annots[0].as_reference().unwrap(), existing_id);
    }
}
