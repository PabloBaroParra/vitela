//! The values `Engine`'s extract and split surface hands to JavaScript (B26
//! T-254).

use wasm_bindgen::prelude::*;

use crate::model::SplitPartPlan;

/// One part of a split: an inclusive run of zero-based positions, and the
/// file the core names it (`"report-part1.pdf"`).
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct SplitPart {
    plan: SplitPartPlan,
}

impl From<SplitPartPlan> for SplitPart {
    fn from(plan: SplitPartPlan) -> Self {
        SplitPart { plan }
    }
}

#[wasm_bindgen]
impl SplitPart {
    /// The part's first page, zero-based.
    #[wasm_bindgen(getter)]
    pub fn first(&self) -> u32 {
        self.plan.first
    }

    /// The part's last page, zero-based and inclusive.
    #[wasm_bindgen(getter)]
    pub fn last(&self) -> u32 {
        self.plan.last
    }

    /// The file it is written under: always one path component.
    #[wasm_bindgen(getter, js_name = fileName)]
    pub fn file_name(&self) -> String {
        self.plan.file_name.clone()
    }
}
