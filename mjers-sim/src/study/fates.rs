//! What becomes of every tile that enters a hand.

use mjers_bot::efficiency::connectivity;
use mjers_game::game::Game;
use mjers_game::trace::Event;
use mjers_hand::meld::{
    KongKind,
    Meld,
};
use mjers_rules::seat::Seat;
use mjers_rules::settlement::WinSource;
use mjers_tile::tile::Tile;
use serde::{
    Deserialize,
    Serialize,
};

/// What happened to every copy of one tile kind that entered a hand.
/// `entered = discarded + melded + in_win + held_at_end`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileFates {
    /// Dealt or drawn into a hand.
    pub entered: u64,
    pub discarded: u64,
    /// Of the discards: claimed into another player's meld.
    pub discards_claimed: u64,
    /// Of the discards: won on.
    pub discards_won_on: u64,
    /// Moved from the holder's own hand into one of their melds.
    pub melded: u64,
    /// Still concealed in the winner's completed hand.
    pub in_win: u64,
    /// Still concealed in a non-winner's hand when the hand ended.
    pub held_at_end: u64,
    /// Of those held at the end: with no copy or same-suit neighbour within two ranks.
    pub isolated_at_end: u64,
}

impl TileFates {
    pub(super) fn merge(
        &mut self,
        o: &TileFates,
    ) {
        self.entered += o.entered;
        self.discarded += o.discarded;
        self.discards_claimed += o.discards_claimed;
        self.discards_won_on += o.discards_won_on;
        self.melded += o.melded;
        self.in_win += o.in_win;
        self.held_at_end += o.held_at_end;
        self.isolated_at_end += o.isolated_at_end;
    }
}

/// Adds one finished hand's tile movements to `fates_by_tile`.
pub(super) fn record_fates(
    fates_by_tile: &mut [TileFates],
    game: &Game,
) {
    let mut last_discard = None;
    let mut won_on_discard = None;
    for event in game.events() {
        match *event {
            Event::Deal { tiles, .. } => tiles.tiles().for_each(|t| update(fates_by_tile, t, |f| f.entered += 1)),
            Event::Draw { tile, .. } => update(fates_by_tile, tile, |f| f.entered += 1),
            Event::Discard { tile, .. } => {
                update(fates_by_tile, tile, |f| f.discarded += 1);
                last_discard = Some(tile);
            },
            Event::Claim { meld, .. } => {
                let Some(claimed) = last_discard else { continue };
                update(fates_by_tile, claimed, |f| f.discards_claimed += 1);
                let mut own = meld.tiles();
                own.remove(claimed);
                own.tiles().for_each(|t| update(fates_by_tile, t, |f| f.melded += 1));
            },
            Event::Kong {
                meld: Meld::Kong(tile, KongKind::Concealed),
                ..
            } => update(fates_by_tile, tile, |f| f.melded += 4),
            Event::Kong { meld, .. } => update(fates_by_tile, meld.tile(), |f| f.melded += 1),
            Event::Win {
                source: WinSource::Discard(_), tile, ..
            } => {
                update(fates_by_tile, tile, |f| f.discards_won_on += 1);
                won_on_discard = Some(tile);
            },
            Event::Win { .. } | Event::Bonus { .. } | Event::Exhausted => {},
        }
    }
    let winner = game.result().and_then(|r| r.win).map(|w| w.winner);
    for seat in Seat::ALL {
        let mut concealed = *game.hand(seat).concealed();
        if Some(seat) == winner {
            if let Some(tile) = won_on_discard {
                concealed.remove(tile);
            }
            concealed.tiles().for_each(|t| update(fates_by_tile, t, |f| f.in_win += 1));
            continue;
        }
        for tile in concealed.tiles() {
            let isolated = connectivity(&concealed, tile) == 0;
            update(fates_by_tile, tile, |f| {
                f.held_at_end += 1;
                f.isolated_at_end += isolated as u64;
            });
        }
    }
}

fn update(
    fates_by_tile: &mut [TileFates],
    tile: Tile,
    change: impl FnOnce(&mut TileFates),
) {
    if let Some(fates) = fates_by_tile.get_mut(tile.index()) {
        change(fates);
    }
}
