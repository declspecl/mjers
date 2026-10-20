//! Versioned neural-policy inputs, shared by training and inference.
//!
//! Only an `Observation` enters the encoder. Seats are relative to its owner:
//! self, next, opposite, previous. Version 1 describes board state across the
//! engine's rule variants, summarizing discard history rather than retaining
//! the full sequence. Rules are external model configuration, not inputs;
//! train and evaluate a model under its recorded ruleset.

#[cfg(test)]
mod tests;

use crate::game::action::{
    Action,
    Offer,
};
use crate::game::observation::{
    Decision,
    Observation,
};
use crate::hand::meld::{
    KongKind,
    Meld,
};
use crate::tile::KINDS;
use crate::tile::counts::TileCounts;

pub const VERSION: u32 = 1;
pub const ACTIONS: usize = 110;
pub const PLANES: usize = 50;
pub const CONTEXT: usize = 22;

// Plane offsets; each count group uses four "at least n copies" planes.
const OWN: usize = 0;
const PUBLIC: usize = 4;
const DISCARDS: usize = 8;
const CLAIMED: usize = 24;
const RECENCY: usize = 28;
const MELDS: usize = 32;
const OFFERED: usize = 48;
const DRAWN: usize = 49;

#[derive(Debug, Clone, PartialEq)]
pub struct EncodedObservation {
    pub tiles: [[f32; KINDS]; PLANES],
    pub context: [f32; CONTEXT],
    pub action_mask: [bool; ACTIONS],
}

/// Fixed action index. Chow indices describe the offered tile's position in
/// the sequence (low, middle, high). Invalid chow/decision pairs have no index.
pub fn action_index(
    action: Action,
    decision: Decision,
) -> Option<usize> {
    Some(match action {
        Action::Discard(tile) => tile.index(),
        Action::ConcealedKong(tile) => 34 + tile.index(),
        Action::UpgradeKong(tile) => 68 + tile.index(),
        Action::Chow(lowest) => {
            let Decision::Claim { tile, .. } = decision else { return None };
            let offset = tile.index().checked_sub(lowest.index())?;
            if offset > 2 || tile.suit().is_none() || tile.suit() != lowest.suit() || Meld::chow(lowest).is_none() {
                return None;
            }
            102 + offset
        },
        Action::Pong => 105,
        Action::Kong => 106,
        Action::Win => 107,
        Action::SelfDrawWin => 108,
        Action::Pass => 109,
    })
}

/// Decodes only legal actions, so a masked index cannot enter the engine.
pub fn legal_action(
    observation: &Observation<'_>,
    index: usize,
) -> Option<Action> {
    observation.legal_actions().iter().copied().find(|&action| action_index(action, observation.decision()) == Some(index))
}

pub fn encode(observation: &Observation<'_>) -> EncodedObservation {
    let mut encoded = EncodedObservation {
        tiles: [[0.0; KINDS]; PLANES],
        context: [0.0; CONTEXT],
        action_mask: [false; ACTIONS],
    };
    let owner = observation.seat();
    count_planes(&mut encoded.tiles, OWN, observation.hand().concealed());
    count_planes(&mut encoded.tiles, PUBLIC, observation.public_tiles());

    let mut discards_by_seat = [TileCounts::new(); 4];
    let history_len = observation.discards().len().max(1) as f32;
    for (i, discard) in observation.discards().iter().enumerate() {
        let relative = discard.seat.turns_after(owner) as usize;
        let tile = discard.tile.index();
        discards_by_seat[relative].add(discard.tile);
        encoded.tiles[CLAIMED + relative][tile] += f32::from(discard.claimed) / 4.0;
        // Last occurrence per seat and tile: later discards approach 1.
        encoded.tiles[RECENCY + relative][tile] = (i + 1) as f32 / history_len;
    }
    for (relative, seat) in std::iter::once(owner).chain(owner.others()).enumerate() {
        count_planes(&mut encoded.tiles, DISCARDS + 4 * relative, &discards_by_seat[relative]);
        for meld in observation.melds(seat) {
            let kind = match meld {
                Meld::Chow(_) => 0,
                Meld::Pong(_) => 1,
                Meld::Kong(_, KongKind::Exposed) => 2,
                Meld::Kong(_, KongKind::Concealed) => 3,
            };
            // Chows are anchored at their lowest tile; duplicate chows add.
            encoded.tiles[MELDS + 4 * relative + kind][meld.tile().index()] += 0.25;
        }
        encoded.context[relative] = observation.bonus(seat).len() as f32 / 8.0;
        encoded.context[4 + relative] = observation.concealed_count(seat) as f32 / 14.0;
        encoded.context[8 + relative] = f32::from(seat == observation.dealer());
    }
    match observation.decision() {
        Decision::Turn { after_claim, drawn } => {
            encoded.context[16] = 1.0;
            encoded.context[19] = f32::from(after_claim);
            if let Some(tile) = drawn {
                encoded.tiles[DRAWN][tile.index()] = 1.0;
            }
        },
        Decision::Claim { tile, from, offer } => {
            encoded.tiles[OFFERED][tile.index()] = 1.0;
            encoded.context[12 + from.turns_after(owner) as usize] = 1.0;
            encoded.context[match offer {
                Offer::Discard => 17,
                Offer::KongUpgrade => 18,
            }] = 1.0;
        },
        Decision::None => {},
    }
    encoded.context[20] = observation.live_wall() as f32 / 144.0;
    encoded.context[21] = observation.wall_remaining() as f32 / 144.0;
    for &action in observation.legal_actions().iter() {
        if let Some(index) = action_index(action, observation.decision()) {
            encoded.action_mask[index] = true;
        }
    }
    encoded
}

fn count_planes(
    planes: &mut [[f32; KINDS]; PLANES],
    start: usize,
    counts: &TileCounts,
) {
    for (threshold, plane) in planes[start..start + 4].iter_mut().enumerate() {
        for (value, &count) in plane.iter_mut().zip(counts.as_array()) {
            *value = f32::from(count as usize > threshold);
        }
    }
}
