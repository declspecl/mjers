//! Speed-oriented evaluations of a hand, shared by bots, the advisor, and studies.

use mjers_game::action::Action;
use mjers_hand::hand::Hand;
use mjers_hand::shanten::MAX_SHANTEN;
use mjers_tile::counts::TileCounts;
use mjers_tile::tile::{
    SUITED_KINDS,
    Tile,
};

/// The hand after a turn-time kong declaration, before its replacement draw.
pub fn after_kong(
    hand: &Hand,
    action: Action,
) -> Option<Hand> {
    let mut after = *hand;
    match action {
        Action::ConcealedKong(t) => after.declare_concealed_kong(t).ok()?,
        Action::UpgradeKong(t) => after.upgrade_kong(t).ok()?,
        _ => return None,
    };
    Some(after)
}

/// The hand right after claiming `tile` (before discarding or replacing).
pub fn after_claim(
    hand: &Hand,
    tile: Tile,
    action: Action,
) -> Option<Hand> {
    let mut after = *hand;
    match action {
        Action::Pong => after.claim_pong(tile).ok()?,
        Action::Kong => after.claim_kong(tile).ok()?,
        Action::Chow(lowest) => after.claim_chow(lowest, tile).ok()?,
        _ => return None,
    };
    Some(after)
}

/// Shanten after the best discard from a hand awaiting discard.
pub fn best_discard_shanten(hand: &Hand) -> i8 {
    hand.concealed()
        .kinds()
        .filter_map(|t| {
            let mut after = *hand;
            after.discard(t).ok().map(|()| after.shanten())
        })
        .min()
        .unwrap_or(MAX_SHANTEN)
}

/// The fastest discard ignoring danger: lowest shanten, most live improving
/// tiles, then least connected. `None` for an empty hand.
pub fn best_discard(
    hand: &Hand,
    visible: &TileCounts,
) -> Option<Tile> {
    let options = hand.discard_options(visible);
    let best = options.first()?.ukeire;
    options
        .iter()
        .filter(|o| o.ukeire.shanten == best.shanten && o.ukeire.total() == best.total())
        .map(|o| o.tile)
        .min_by_key(|&t| (connectivity(hand.concealed(), t), std::cmp::Reverse(t.is_honor()), t))
}

/// How many held tiles could combine with `tile`: copies and suit neighbours
/// within two ranks.
pub fn connectivity(
    concealed: &TileCounts,
    tile: Tile,
) -> u8 {
    let i = tile.index();
    let copies = concealed.get(tile).saturating_sub(1);
    if i >= SUITED_KINDS {
        return copies;
    }
    let rank = i % 9;
    let neighbours = (rank.saturating_sub(2)..=(rank + 2).min(8))
        .filter(|&r| r != rank)
        .map(|r| concealed.as_array()[i - rank + r])
        .sum::<u8>();
    copies + neighbours
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hand(notation: &str) -> Hand {
        Hand::new(notation.parse().unwrap())
    }

    fn tile(notation: &str) -> Tile {
        notation.parse().unwrap()
    }

    #[test]
    fn discards_the_isolated_honor() {
        assert_eq!(best_discard(&hand("123m456p78s1122z9m5z"), &TileCounts::new()), Some(tile("5z")));
    }

    #[test]
    fn breaks_ties_by_connectivity() {
        // 1z and 9s are both useless; 9s touches 7s/8s.
        assert_eq!(best_discard(&hand("123m456p789s78s11z1z"), &TileCounts::new()), Some(tile("1z")));
    }

    #[test]
    fn visible_tiles_steer_the_discard() {
        // Keeping 45m (waits 3m/6m) vs 78m (6m/9m): if 3m is dead, keep 78m.
        let dead_three: TileCounts = "3333m".parse().unwrap();
        let discard = best_discard(&hand("45m78m456p789s111z"), &dead_three);
        assert!(discard == Some(tile("4m")) || discard == Some(tile("5m")), "{discard:?}");
    }

    #[test]
    fn empty_hands_have_no_discard() {
        assert_eq!(best_discard(&hand(""), &TileCounts::new()), None);
    }

    #[test]
    fn connectivity_counts_neighbours_in_suit_only() {
        let counts: TileCounts = "79m1p11z".parse().unwrap();
        assert_eq!(connectivity(&counts, tile("9m")), 1);
        assert_eq!(connectivity(&counts, tile("1p")), 0);
        assert_eq!(connectivity(&counts, tile("1z")), 1);
    }
}
