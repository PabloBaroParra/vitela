//! Form-field values as they cross into JavaScript (B26 T-242).
//!
//! Field geometry is `f64` in the core, like annotation geometry, so it
//! crosses the same way ([`super::annotation::rect_values`]); a radio
//! group's buttons cross as two parallel lists, their export values and
//! their boxes flattened `x, y, width, height` per button.

use pdf_document::Rect;

use super::annotation::rect_values;

/// A radio group's buttons as (export values, `[x0, y0, w0, h0, x1, ...]`),
/// in the group's order.
pub fn radio_options(options: Vec<(String, Rect)>) -> (Vec<String>, Vec<f64>) {
    let mut values = Vec::with_capacity(options.len());
    let mut rects = Vec::with_capacity(options.len() * 4);
    for (value, rect) in options {
        values.push(value);
        rects.extend(rect_values(rect));
    }
    (values, rects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::annotation::rect;

    #[test]
    fn radio_buttons_cross_as_parallel_values_and_boxes() {
        let (values, rects) = radio_options(vec![
            ("basic".to_string(), rect(72.0, 500.0, 18.0, 18.0)),
            ("pro".to_string(), rect(132.0, 500.0, 18.0, 18.0)),
        ]);
        assert_eq!(values, ["basic", "pro"]);
        assert_eq!(rects, [72.0, 500.0, 18.0, 18.0, 132.0, 500.0, 18.0, 18.0]);
        assert_eq!(radio_options(Vec::new()), (Vec::new(), Vec::new()));
    }
}
