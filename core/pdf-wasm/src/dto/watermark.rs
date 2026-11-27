//! The value `Engine`'s watermark surface hands to JavaScript (B26 T-256).

use wasm_bindgen::prelude::*;

use crate::model::WatermarkDefaults;

/// The core's starting looks for a text and a picture stamp. Opacities are
/// `0..=1`, angles counter-clockwise degrees, `behind` paints the stamp
/// under the page's content.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub struct WatermarkLook {
    look: WatermarkDefaults,
}

impl From<WatermarkDefaults> for WatermarkLook {
    fn from(look: WatermarkDefaults) -> Self {
        WatermarkLook { look }
    }
}

#[wasm_bindgen]
impl WatermarkLook {
    /// In points.
    #[wasm_bindgen(getter, js_name = fontSize)]
    pub fn font_size(&self) -> f64 {
        self.look.font_size
    }

    #[wasm_bindgen(getter, js_name = textOpacity)]
    pub fn text_opacity(&self) -> f64 {
        self.look.text_opacity
    }

    #[wasm_bindgen(getter, js_name = textRotationDegrees)]
    pub fn text_rotation_degrees(&self) -> f64 {
        self.look.text_rotation_degrees
    }

    #[wasm_bindgen(getter, js_name = textBehind)]
    pub fn text_behind(&self) -> bool {
        self.look.text_behind
    }

    /// The share of the page a picture may cover, `0..=1`.
    #[wasm_bindgen(getter, js_name = pictureScale)]
    pub fn picture_scale(&self) -> f64 {
        self.look.picture_scale
    }

    #[wasm_bindgen(getter, js_name = pictureOpacity)]
    pub fn picture_opacity(&self) -> f64 {
        self.look.picture_opacity
    }

    #[wasm_bindgen(getter, js_name = pictureRotationDegrees)]
    pub fn picture_rotation_degrees(&self) -> f64 {
        self.look.picture_rotation_degrees
    }

    #[wasm_bindgen(getter, js_name = pictureBehind)]
    pub fn picture_behind(&self) -> bool {
        self.look.picture_behind
    }
}
