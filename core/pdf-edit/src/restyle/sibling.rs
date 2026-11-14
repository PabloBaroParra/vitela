//! The other faces of a run's font family that the page already carries:
//! `Helvetica-Bold` beside `Helvetica`, `ABCDEF+Arial-BoldMT` beside
//! `ABCDEF+ArialMT`.
//!
//! A sibling is only usable if it can show the run's text: an embedded
//! subset holds the glyphs its producer used and nothing else, so its
//! encoding is asked for every character ([`FontInfo::encode`] — the same
//! coverage check a retype makes) and a face that lacks one is passed over.

use lopdf::{Dictionary, Document, Object};

use crate::edit::{format_number, literal_string};
use crate::encoding::fallback::{font_look, is_fallback_resource, Look};
use crate::encoding::{resolve_font, resolved_name, FontInfo};
use crate::insert::dereferenced_dict;
use crate::parse::{tokenize, Operand};

/// One face of the run's family, with the run's text already encoded in it.
pub(crate) struct Face {
    /// Its key in the scope's `/Resources /Font`.
    pub(crate) name: String,
    pub(crate) look: Look,
    pub(crate) info: FontInfo,
    pub(crate) codes: Vec<u8>,
    pub(crate) array: Option<(Vec<u8>, f64)>,
}

/// A `TJ` array's strings re-encoded from `own` into `face`, its kerning
/// (often the word gaps) kept, and the kerning's sum; `None` when one cannot be.
fn array_in(items: &[Operand], own: &FontInfo, face: &FontInfo) -> Option<(Vec<u8>, f64)> {
    let (mut array, mut kerning) = (b"[".to_vec(), 0.0);
    for item in items {
        match item.as_bytes() {
            Some(codes) => array.extend(literal_string(&face.encode(&own.decode(codes)).ok()?)),
            None => {
                kerning += item.as_f64().unwrap_or(0.0);
                let number = format_number(item.as_f64().unwrap_or(0.0));
                array.extend(format!(" {number} ").into_bytes());
            }
        }
    }
    array.push(b']');
    Some((array, kerning))
}

/// Words a font name ends in that name a face, not a family. Stripped
/// repeatedly, so `BoldItalicMT` goes in three steps.
const STYLE_SUFFIXES: &[&str] = &[
    "mt", "ps", "bold", "italic", "oblique", "regular", "book", "normal",
];

/// The family a `/BaseFont` belongs to, lowercase: the subset tag
/// (`ABCDEF+`) dropped, everything after the first `-` or `,` dropped, then
/// any style word left on the end (`ArialMT`, `TimesNewRomanPSMT`).
pub(crate) fn family_of(base_font: &str) -> String {
    let name = match base_font.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|b| b.is_ascii_uppercase()) => rest,
        _ => base_font,
    };
    let head = name.split(['-', ',']).next().unwrap_or(name);
    let mut family = head.to_ascii_lowercase();
    while let Some(stripped) = STYLE_SUFFIXES.iter().find_map(|suffix| {
        family
            .strip_suffix(suffix)
            .filter(|rest| !rest.is_empty())
            .map(str::to_string)
    }) {
        family = stripped;
    }
    family
}

/// The run's own font, by its resource name in `resources`.
pub(crate) fn own_face(
    document: &Document,
    resources: &Dictionary,
    name: &str,
) -> Option<(Dictionary, Look)> {
    let font = font_entries(document, resources)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, font)| font)?;
    let look = font_look(document, &font);
    Some((font, look))
}

/// Every other font in `resources` of the same family as `own`, which can
/// show all of `text`, string by string for a `TJ` `array`. Fallbacks this crate wrote are out:
/// a run in one is a stretch of a line in another font.
pub(crate) fn siblings(
    document: &Document,
    resources: &Dictionary,
    own_name: &str,
    own: &Dictionary,
    text: &str,
    array: Option<&[u8]>,
) -> Vec<Face> {
    let items = match array.map(|operand| tokenize(&[operand, b" TJ"].concat())) {
        Some(Ok(operations)) => match operations.first().and_then(|op| op.operands.first()) {
            Some(Operand::Array(items)) => Some(items.clone()),
            _ => return Vec::new(),
        },
        Some(Err(_)) => return Vec::new(),
        None => None,
    };
    let own_info = resolve_font(document, own, own_name).ok();
    let family = family_of(&resolved_name(document, own, b"BaseFont").unwrap_or_default());
    if family.is_empty() {
        return Vec::new();
    }
    let mut faces: Vec<Face> = font_entries(document, resources)
        .into_iter()
        .filter(|(name, _)| name != own_name && !is_fallback_resource(name))
        .filter(|(_, font)| {
            family_of(&resolved_name(document, font, b"BaseFont").unwrap_or_default()) == family
        })
        .filter_map(|(name, font)| {
            let info = resolve_font(document, &font, &name).ok()?;
            let codes = info.encode(text).ok()?;
            let array = match &items {
                Some(items) => Some(array_in(items, own_info.as_ref()?, &info)?),
                None => None,
            };
            Some(Face {
                look: font_look(document, &font),
                name,
                info,
                codes,
                array,
            })
        })
        .collect();
    // Resource order is a dictionary's order; a stable pick has to come from
    // the names.
    faces.sort_by(|left, right| left.name.cmp(&right.name));
    faces
}

/// `(resource name, font dictionary)` for every font in `resources`.
fn font_entries(document: &Document, resources: &Dictionary) -> Vec<(String, Dictionary)> {
    let Some(fonts) = resources
        .get(b"Font")
        .ok()
        .and_then(|fonts| dereferenced_dict(document, fonts))
    else {
        return Vec::new();
    };
    fonts
        .iter()
        .filter_map(|(name, font)| {
            let font = match font {
                Object::Reference(_) | Object::Dictionary(_) => dereferenced_dict(document, font)?,
                _ => return None,
            };
            Some((String::from_utf8_lossy(name).into_owned(), font))
        })
        .collect()
}
