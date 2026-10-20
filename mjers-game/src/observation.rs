//! What one seat may see. Policies receive only this.

use mjers_hand::hand::{
    BonusSet,
    Hand,
};
use mjers_hand::meld::Meld;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;
use mjers_tile::counts::TileCounts;
use mjers_tile::tile::Tile;

use crate::action::{
    Actions,
    Offer,
    Phase,
};
use crate::game::{
    Discard,
    Game,
};

/// The decision a seat currently faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Turn {
        after_claim: bool,
        drawn: Option<Tile>,
    },
    Claim {
        tile: Tile,
        from: Seat,
        offer: Offer,
    },
    /// Waiting on other seats, or the hand is over.
    None,
}

/// A read-only view of `game` limited to public information plus `seat`'s
/// own hand. Other hands' concealed tiles and the wall order are unreachable.
#[derive(Clone, Copy)]
pub struct Observation<'a> {
    game: &'a Game,
    seat: Seat,
}

impl Game {
    pub fn observe(
        &self,
        seat: Seat,
    ) -> Observation<'_> {
        Observation { game: self, seat }
    }
}

impl<'a> Observation<'a> {
    pub fn seat(&self) -> Seat {
        self.seat
    }

    pub fn rules(&self) -> &'a Ruleset {
        self.game.rules()
    }

    pub fn dealer(&self) -> Seat {
        self.game.dealer()
    }

    pub fn hand(&self) -> &'a Hand {
        self.game.hand(self.seat)
    }

    /// Any seat's declared melds (concealed kongs are declared face up).
    pub fn melds(
        &self,
        seat: Seat,
    ) -> impl Iterator<Item = Meld> + 'a {
        self.game.hand(seat).melds()
    }

    pub fn bonus(
        &self,
        seat: Seat,
    ) -> BonusSet {
        self.game.hand(seat).bonus()
    }

    /// How many concealed tiles a seat holds, without revealing which.
    pub fn concealed_count(
        &self,
        seat: Seat,
    ) -> usize {
        self.game.hand(seat).concealed().len()
    }

    pub fn discards(&self) -> &'a [Discard] {
        self.game.discards()
    }

    /// Tiles visible to everyone: discards and declared melds.
    pub fn public_tiles(&self) -> &'a TileCounts {
        self.game.public_tiles()
    }

    /// Tiles physically left in the wall, flower wall included.
    pub fn wall_remaining(&self) -> usize {
        self.game.wall().remaining()
    }

    /// Normal draws left before the hand ends with no winner.
    pub fn live_wall(&self) -> usize {
        self.game.wall().live_remaining()
    }

    pub fn decision(&self) -> Decision {
        if !self.game.to_act().contains(self.seat) {
            return Decision::None;
        }
        match *self.game.phase() {
            Phase::Turn { after_claim, .. } => Decision::Turn {
                after_claim,
                drawn: self.game.last_draw(),
            },
            Phase::Claim { tile, from, offer } => Decision::Claim { tile, from, offer },
            Phase::Ended(_) => Decision::None,
        }
    }

    pub fn legal_actions(&self) -> Actions {
        self.game.legal_actions(self.seat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::testing::*;

    const HANDS: [&str; 4] = ["123456789m1234z", "123456789p1119s", "1234567s99s5566z", "78s222333444z66z"];

    #[test]
    fn turn_shows_own_hand_and_draw() {
        let game = game(HANDS, "9s", "");
        let obs = game.observe(S[0]);
        assert_eq!(
            obs.decision(),
            Decision::Turn {
                after_claim: false,
                drawn: Some(tile("9s"))
            }
        );
        assert_eq!(obs.hand().concealed().to_string(), "123456789m9s1234z");
        assert_eq!(obs.concealed_count(S[2]), 13);
        assert_eq!(obs.live_wall(), game.wall().live_remaining());
        assert_eq!(game.observe(S[1]).decision(), Decision::None);
    }

    #[test]
    fn claim_window_shows_the_offer() {
        let mut game = game(HANDS, "9s", "");
        game.apply(S[0], Action::Discard(tile("9s"))).unwrap();
        let obs = game.observe(S[2]);
        assert_eq!(
            obs.decision(),
            Decision::Claim {
                tile: tile("9s"),
                from: S[0],
                offer: Offer::Discard
            }
        );
        assert_eq!(obs.legal_actions(), [Action::Pong, Action::Pass]);
        assert_eq!(obs.public_tiles().to_string(), "9s");
        assert_eq!(obs.discards().len(), 1);
        game.apply(S[1], Action::Pass).unwrap();
        game.apply(S[3], Action::Pass).unwrap();
        game.apply(S[2], Action::Pong).unwrap();
        let other = game.observe(S[0]);
        assert_eq!(other.melds(S[2]).collect::<Vec<_>>(), [Meld::Pong(tile("9s"))]);
        assert_eq!(other.concealed_count(S[2]), 11);
    }
}
