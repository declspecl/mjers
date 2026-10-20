//! Types for the plain objects crossing the JavaScript boundary.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = include_str!("../../npm/types.d.ts");

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Action")]
    pub type ActionValue;
    #[wasm_bindgen(typescript_type = "Action[]")]
    pub type ActionsValue;
    #[wasm_bindgen(typescript_type = "Seat[]")]
    pub type SeatsValue;
    #[wasm_bindgen(typescript_type = "Ruleset")]
    pub type RulesetValue;
    #[wasm_bindgen(typescript_type = "Observation")]
    pub type ObservationValue;
    #[wasm_bindgen(typescript_type = "AdviceOptions")]
    pub type AdviceOptionsValue;
    #[wasm_bindgen(typescript_type = "Advice")]
    pub type AdviceValue;
    #[wasm_bindgen(typescript_type = "Analysis")]
    pub type AnalysisValue;
    #[wasm_bindgen(typescript_type = "HandResult | null")]
    pub type ResultValue;
    #[wasm_bindgen(typescript_type = "Settlement | null")]
    pub type SettlementValue;
}
