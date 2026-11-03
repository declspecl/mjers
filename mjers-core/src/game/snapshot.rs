//! Owned snapshots built exclusively through the core's observation interface.

use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};

use crate::game::action::{
    Action,
    GameError,
    Offer,
};
use crate::game::observation::Decision;
use crate::game::position::{
    Position,
    ResumePoint,
};
use crate::game::rng::Rng;
use crate::game::wall::{
    WALL_SIZE,
    WallTile,
};
use crate::game::{
    Discard,
    Game,
};
use crate::hand::meld::Meld;
use crate::rules::Ruleset;
use crate::rules::seat::Seat;
use crate::tile::counts::{
    COPIES,
    TileCounts,
};
use crate::tile::{
    Bonus,
    Tile,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    seat: Seat,
    dealer: Seat,
    rules: Ruleset,
    hand: Hand,
    players: [Player; 4],
    discards: Vec<Discard>,
    public_tiles: TileCounts,
    wall_remaining: usize,
    live_wall: usize,
    to_act: Vec<Seat>,
    legal_actions: Vec<Action>,
    decision: DecisionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hand {
    concealed: TileCounts,
    melds: Vec<Meld>,
    bonus: Vec<Bonus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Player {
    seat: Seat,
    concealed_count: usize,
    melds: Vec<Meld>,
    bonus: Vec<Bonus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DecisionView {
    Turn { after_claim: bool, drawn: Option<Tile> },
    Claim { tile: Tile, from: Seat, offer: Offer },
    None,
}

impl Snapshot {
    /// Reconstruct a simulation from visible information only. The hidden tiles
    /// are sampled, never restored from the actual multiplayer game or its seed.
    pub fn sample_game(
        &self,
        seed: u64,
    ) -> Result<Game, GameError> {
        let invalid = || GameError::InvalidPosition("inconsistent observation");
        if !self.to_act.contains(&self.seat) || self.wall_remaining > WALL_SIZE {
            return Err(invalid());
        }
        let point = match self.decision {
            DecisionView::Turn { after_claim, drawn } => ResumePoint::Turn {
                seat: self.seat,
                after_claim,
                last_draw: drawn,
            },
            DecisionView::Claim { tile, from, offer } => ResumePoint::Claim { tile, from, offer },
            DecisionView::None => return Err(invalid()),
        };
        let mut rng = Rng::seed(seed);
        let mut hidden = Vec::new();
        for tile in Tile::all() {
            let visible = self.hand.concealed.get(tile) + self.public_tiles.get(tile);
            if visible > COPIES {
                return Err(invalid());
            }
            hidden.extend(std::iter::repeat_n(tile, (COPIES - visible) as usize));
        }
        rng.shuffle(&mut hidden);
        let mut concealed = [TileCounts::new(); 4];
        concealed[self.seat.index()] = self.hand.concealed;
        if let DecisionView::Claim {
            tile,
            from,
            offer: Offer::KongUpgrade,
        } = self.decision
        {
            let index = hidden.iter().position(|&t| t == tile).ok_or_else(invalid)?;
            hidden.swap_remove(index);
            concealed[from.index()].add(tile);
        }
        for seat in Seat::ALL {
            let player = &self.players[seat.index()];
            if player.seat != seat || player.concealed_count > 14 {
                return Err(invalid());
            }
            if seat == self.seat {
                continue;
            }
            while concealed[seat.index()].len() < player.concealed_count {
                concealed[seat.index()].add(hidden.pop().ok_or_else(invalid)?);
            }
        }
        let held: Vec<Bonus> = self.players.iter().flat_map(|p| p.bonus.iter().copied()).collect();
        let mut wall: Vec<WallTile> = hidden.into_iter().map(WallTile::Tile).chain(Bonus::all().filter(|b| !held.contains(b)).map(WallTile::Bonus)).collect();
        rng.shuffle(&mut wall);
        if wall.len() != self.wall_remaining {
            return Err(invalid());
        }
        let mut hands = [crate::hand::Hand::new(TileCounts::new()); 4];
        for seat in Seat::ALL {
            let player = &self.players[seat.index()];
            let mut bonus = crate::hand::BonusSet::default();
            for &tile in &player.bonus {
                bonus.insert(tile);
            }
            hands[seat.index()] = crate::hand::Hand::from_parts(concealed[seat.index()], player.melds.iter().copied(), bonus).map_err(|_| invalid())?;
        }
        let game = Game::resume(
            Arc::new(self.rules.clone()),
            Position {
                hands,
                wall,
                dealer: self.dealer,
                discards: self.discards.clone(),
                public: self.public_tiles,
                point,
            },
        )?;
        let obs = game.observe(self.seat);
        if Snapshot::new(&game, self.seat).hand != self.hand || obs.legal_actions().to_vec() != self.legal_actions || obs.live_wall() != self.live_wall {
            return Err(invalid());
        }
        Ok(game)
    }

    pub fn new(
        game: &Game,
        seat: Seat,
    ) -> Self {
        let obs = game.observe(seat);
        Self {
            seat,
            dealer: obs.dealer(),
            rules: obs.rules().clone(),
            hand: Hand {
                concealed: *obs.hand().concealed(),
                melds: obs.hand().melds().collect(),
                bonus: obs.hand().bonus().iter().collect(),
            },
            players: Seat::ALL.map(|seat| Player {
                seat,
                concealed_count: obs.concealed_count(seat),
                melds: obs.melds(seat).collect(),
                bonus: obs.bonus(seat).iter().collect(),
            }),
            discards: obs.discards().to_vec(),
            public_tiles: *obs.public_tiles(),
            wall_remaining: obs.wall_remaining(),
            live_wall: obs.live_wall(),
            to_act: game.to_act().iter().collect(),
            legal_actions: obs.legal_actions().to_vec(),
            decision: match obs.decision() {
                Decision::Turn { after_claim, drawn } => DecisionView::Turn { after_claim, drawn },
                Decision::Claim { tile, from, offer } => DecisionView::Claim { tile, from, offer },
                Decision::None => DecisionView::None,
            },
        }
    }
}

impl Hand {
    pub fn new(hand: &crate::hand::Hand) -> Self {
        Self {
            concealed: *hand.concealed(),
            melds: hand.melds().collect(),
            bonus: hand.bonus().iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::heuristic::HeuristicBot;
    use crate::bot::policy::decide;

    #[test]
    fn snapshots_round_trip_and_reconstruct_only_visible_information() {
        for seed in 0..12 {
            let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]).unwrap();
            let mut bot = HeuristicBot::efficiency();
            for step in 0..60 {
                let Some(seat) = game.to_act().first() else { break };
                let snapshot = Snapshot::new(&game, seat);
                let decoded: Snapshot = serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).unwrap();
                assert_eq!(snapshot, decoded);
                let sampled = decoded.sample_game(9000 + step).unwrap();
                let rebuilt = Snapshot::new(&sampled, seat);
                assert_eq!(snapshot.hand, rebuilt.hand);
                assert_eq!(snapshot.players, rebuilt.players);
                assert_eq!(snapshot.public_tiles, rebuilt.public_tiles);
                assert_eq!(snapshot.discards, rebuilt.discards);
                assert_eq!(snapshot.legal_actions, rebuilt.legal_actions);
                assert_eq!(snapshot.decision, rebuilt.decision);
                assert_eq!(snapshot.wall_remaining, rebuilt.wall_remaining);
                let action = decide(&game, seat, &mut bot);
                game.apply(seat, action).unwrap();
            }
        }
    }

    #[test]
    fn waiting_and_inconsistent_snapshots_are_rejected() {
        let game = Game::new(Arc::new(Ruleset::family()), 42, Seat::ALL[0]).unwrap();
        assert!(Snapshot::new(&game, Seat::ALL[1]).sample_game(1).is_err());
        let mut snapshot = Snapshot::new(&game, Seat::ALL[0]);
        snapshot.wall_remaining = 999;
        assert!(snapshot.sample_game(1).is_err());
    }
}
