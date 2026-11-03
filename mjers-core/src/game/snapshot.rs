//! Owned snapshots built exclusively through the core's observation interface.

use serde::Serialize;

use crate::game::action::{
    Action,
    Offer,
};
use crate::game::observation::Decision;
use crate::game::{
    Discard,
    Game,
};
use crate::hand::meld::Meld;
use crate::rules::Ruleset;
use crate::rules::seat::Seat;
use crate::tile::counts::TileCounts;
use crate::tile::{
    Bonus,
    Tile,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hand {
    concealed: TileCounts,
    melds: Vec<Meld>,
    bonus: Vec<Bonus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Player {
    seat: Seat,
    concealed_count: usize,
    melds: Vec<Meld>,
    bonus: Vec<Bonus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DecisionView {
    Turn { after_claim: bool, drawn: Option<Tile> },
    Claim { tile: Tile, from: Seat, offer: Offer },
    None,
}

impl Snapshot {
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
