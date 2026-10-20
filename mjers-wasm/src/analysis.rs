//! Hand analysis with the same notation and ranking as `mj analyze`.

use mjers_core::hand::shanten::shanten;
use mjers_core::hand::ukeire::{
    Ukeire,
    discard_options,
    ukeire,
};
use mjers_core::tile::Tile;
use mjers_core::tile::counts::{
    COPIES,
    TileCounts,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::types::AnalysisValue;
use crate::{
    encode,
    error,
};

#[derive(Serialize)]
struct Improvement {
    tile: Tile,
    live: u8,
}

#[derive(Serialize)]
struct Acceptance {
    shanten: i8,
    live_improving: u32,
    improvements: Vec<Improvement>,
}

impl From<Ukeire> for Acceptance {
    fn from(ukeire: Ukeire) -> Self {
        Self {
            shanten: ukeire.shanten,
            live_improving: ukeire.total(),
            improvements: ukeire.improvements().map(|i| Improvement { tile: i.tile, live: i.live }).collect(),
        }
    }
}

#[derive(Serialize)]
struct DiscardOption {
    tile: Tile,
    #[serde(flatten)]
    acceptance: Acceptance,
}

#[derive(Serialize)]
struct Analysis {
    #[serde(flatten)]
    acceptance: Acceptance,
    discards: Vec<DiscardOption>,
}

/// Analyze 13 or 14 tiles, counting three per declared meld. `visible` includes
/// discards and all declared melds (including your own), not concealed tiles.
#[wasm_bindgen]
pub fn analyze(
    concealed: &str,
    melds: Option<usize>,
    visible: Option<String>,
) -> Result<AnalysisValue, JsError> {
    let concealed: TileCounts = concealed.parse().map_err(error)?;
    let visible: TileCounts = visible.unwrap_or_default().parse().map_err(error)?;
    let melds = melds.unwrap_or(0);
    if melds > 4 || !matches!(concealed.len() + 3 * melds, 13 | 14) {
        return Err(JsError::new("a hand holds 13 or 14 tiles counting 3 per meld, with at most 4 melds"));
    }
    if Tile::all().any(|t| concealed.get(t) + visible.get(t) > COPIES) {
        return Err(JsError::new("concealed and visible tiles together exceed four copies of a tile"));
    }
    let size = concealed.len() + 3 * melds;
    let shanten = shanten(&concealed, melds);
    let acceptance = if size == 13 {
        ukeire(&concealed, melds, &visible).into()
    } else {
        Acceptance {
            shanten,
            live_improving: 0,
            improvements: Vec::new(),
        }
    };
    let discards = if size == 14 && shanten != -1 {
        discard_options(&concealed, melds, &visible)
            .into_iter()
            .map(|o| DiscardOption {
                tile: o.tile,
                acceptance: o.ukeire.into(),
            })
            .collect()
    } else {
        Vec::new()
    };
    encode(&Analysis { acceptance, discards })
}
