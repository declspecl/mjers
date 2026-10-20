//! Owned snapshots built exclusively through the core's observation interface.

use mjers_core::game::action::{
    Action,
    Offer,
};
use mjers_core::game::observation::Decision;
use mjers_core::game::{
    Discard,
    Game,
};
use mjers_core::hand::meld::Meld;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use mjers_core::tile::counts::TileCounts;
use mjers_core::tile::{
    Bonus,
    Tile,
};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct Snapshot<'a> {
    seat: Seat,
    dealer: Seat,
    rules: &'a Ruleset,
    hand: Hand,
    players: [Player; 4],
    discards: &'a [Discard],
    public_tiles: &'a TileCounts,
    wall_remaining: usize,
    live_wall: usize,
    to_act: Vec<Seat>,
    legal_actions: Vec<Action>,
    decision: DecisionView,
}

#[derive(Serialize)]
struct Hand {
    concealed: TileCounts,
    melds: Vec<Meld>,
    bonus: Vec<Bonus>,
}

#[derive(Serialize)]
struct Player {
    seat: Seat,
    concealed_count: usize,
    melds: Vec<Meld>,
    bonus: Vec<Bonus>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DecisionView {
    Turn { after_claim: bool, drawn: Option<Tile> },
    Claim { tile: Tile, from: Seat, offer: Offer },
    None,
}

impl<'a> Snapshot<'a> {
    pub(crate) fn new(
        game: &'a Game,
        seat: Seat,
    ) -> Self {
        let obs = game.observe(seat);
        Self {
            seat,
            dealer: obs.dealer(),
            rules: obs.rules(),
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
            discards: obs.discards(),
            public_tiles: obs.public_tiles(),
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
