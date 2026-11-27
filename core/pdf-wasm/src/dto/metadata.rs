//! The value `Engine`'s document-properties surface hands to JavaScript (B26
//! T-255).

use wasm_bindgen::prelude::*;

use crate::model::DocumentProperties;

/// The six text keys of `/Info`; `undefined` is an absent key.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct DocumentPropertiesEntry {
    properties: DocumentProperties,
}

impl From<DocumentProperties> for DocumentPropertiesEntry {
    fn from(properties: DocumentProperties) -> Self {
        DocumentPropertiesEntry { properties }
    }
}

#[wasm_bindgen]
impl DocumentPropertiesEntry {
    #[wasm_bindgen(getter)]
    pub fn title(&self) -> Option<String> {
        self.properties.title.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn author(&self) -> Option<String> {
        self.properties.author.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn subject(&self) -> Option<String> {
        self.properties.subject.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn keywords(&self) -> Option<String> {
        self.properties.keywords.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn creator(&self) -> Option<String> {
        self.properties.creator.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn producer(&self) -> Option<String> {
        self.properties.producer.clone()
    }
}
