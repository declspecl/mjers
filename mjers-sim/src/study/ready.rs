//! Classifying a hand by how it waits when it first becomes ready.

use mjers_hand::hand::Hand;
use mjers_hand::wait::{
    WaitShape,
    wait_shapes,
};
use mjers_tile::counts::{
    COPIES,
    TileCounts,
};
use mjers_tile::tile::Tile;

/// The wait-shape names reported, from most to least flexible.
pub const MULTI_SIDED: &str = "multi-sided (3+ tiles)";
pub const TANKI: &str = "eye (tanki)";

/// How a ready hand waits, by the most flexible role any winning tile plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyShape {
    pub name: &'static str,
    pub waits: usize,
    pub live: u32,
    /// The waited tile, for single-tile eye waits.
    pub tanki_tile: Option<Tile>,
    /// Discards at the table, and events, when the seat became ready.
    pub discards_at: usize,
    pub events_at: usize,
}

pub fn ready_shape(
    hand: &Hand,
    public: &TileCounts,
) -> Option<ReadyShape> {
    let waits = hand.waits();
    if waits.is_empty() {
        return None;
    }
    let concealed = hand.concealed();
    let live = waits.iter().map(|&t| COPIES.saturating_sub(concealed.get(t) + public.get(t)) as u32).sum();
    let best_role = waits.iter().flat_map(|&t| wait_shapes(&concealed.with(t), hand.meld_count(), t)).min();
    let name = if waits.len() >= 3 {
        MULTI_SIDED
    } else {
        match best_role {
            Some(WaitShape::Ryanmen) => "two-sided chow",
            Some(WaitShape::Shanpon) => "pair to pong",
            Some(WaitShape::Kanchan) => "gap chow",
            Some(WaitShape::Penchan) => "edge chow",
            Some(WaitShape::Tanki) | None => TANKI,
        }
    };
    let tanki_tile = match waits.as_slice() {
        [only] if name == TANKI => Some(*only),
        _ => None,
    };
    Some(ReadyShape {
        name,
        waits: waits.len(),
        live,
        tanki_tile,
        discards_at: 0,
        events_at: 0,
    })
}
