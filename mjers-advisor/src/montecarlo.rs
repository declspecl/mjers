//! Estimating each action's value by playing it out on sampled deals.
//!
//! Hidden information (opponents' concealed tiles and the wall order) is
//! resampled consistently with what the seat can see, each candidate action is
//! applied, and bots play the hand out. Every candidate sees the same samples,
//! so differences between them are not luck of the sample.

use std::sync::Arc;

use mjers_bot::policy::play_out;
use mjers_bot::spec::BotSpec;
use mjers_game::action::{
    Action,
    Offer,
};
use mjers_game::game::Game;
use mjers_game::observation::{
    Decision,
    Observation,
};
use mjers_game::position::{
    Position,
    ResumePoint,
};
use mjers_game::rng::{
    Rng,
    derive_seed,
};
use mjers_game::wall::{
    WallTile,
    full_set,
};
use mjers_hand::hand::Hand;
use mjers_rules::seat::Seat;
use mjers_rules::settlement::WinSource;
use mjers_tile::counts::TileCounts;
use serde::{
    Deserialize,
    Serialize,
};

/// One way the hidden tiles could lie, given what `obs` shows.
pub fn sample_position(
    obs: &Observation<'_>,
    rng: &mut Rng,
) -> Position {
    let seat = obs.seat();
    let mut hidden: Vec<WallTile> = full_set().to_vec();
    let mut take = |tile: WallTile| {
        let i = hidden.iter().position(|&t| t == tile).expect("visible tiles come from the set");
        hidden.swap_remove(i);
    };
    for tile in obs.hand().concealed().tiles().chain(obs.public_tiles().tiles()) {
        take(WallTile::Tile(tile));
    }
    for s in Seat::ALL {
        for bonus in obs.bonus(s).iter() {
            take(WallTile::Bonus(bonus));
        }
    }
    let (mut tiles, bonuses): (Vec<WallTile>, Vec<WallTile>) = hidden.into_iter().partition(|t| matches!(t, WallTile::Tile(_)));
    rng.shuffle(&mut tiles);

    let mut concealed: [TileCounts; 4] = Default::default();
    concealed[seat.index()] = *obs.hand().concealed();
    // A player adding to a kong that may be robbed must hold the tile.
    if let Decision::Claim {
        tile,
        from,
        offer: Offer::KongUpgrade,
    } = obs.decision()
    {
        let i = tiles.iter().position(|&t| t == WallTile::Tile(tile)).expect("the kong tile is unseen");
        tiles.swap_remove(i);
        concealed[from.index()].add(tile);
    }
    for other in seat.others() {
        while concealed[other.index()].len() < obs.concealed_count(other) {
            let WallTile::Tile(tile) = tiles.pop().expect("enough hidden tiles") else { unreachable!() };
            concealed[other.index()].add(tile);
        }
    }
    let mut wall: Vec<WallTile> = tiles.into_iter().chain(bonuses).collect();
    rng.shuffle(&mut wall);
    debug_assert_eq!(wall.len(), obs.wall_remaining());

    let point = match obs.decision() {
        Decision::Turn { after_claim, drawn } => ResumePoint::Turn { seat, after_claim, last_draw: drawn },
        Decision::Claim { tile, from, offer } => ResumePoint::Claim { tile, from, offer },
        Decision::None => panic!("{seat} has no decision to sample for"),
    };
    Position {
        hands: Seat::ALL.map(|s| Hand::from_parts(concealed[s.index()], obs.melds(s), obs.bonus(s))),
        wall,
        dealer: obs.dealer(),
        discards: obs.discards().to_vec(),
        public: *obs.public_tiles(),
        point,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    pub action: Action,
    pub samples: u32,
    /// Mean units this seat ends the hand with.
    pub ev: f64,
    pub ev_stderr: f64,
    pub win_rate: f64,
    pub deal_in_rate: f64,
    /// Mean of this action's units minus the first candidate's, on the same
    /// samples, and its standard error. Zero for the first candidate.
    pub difference: f64,
    pub difference_stderr: f64,
}

/// Plays each of `actions` out from `samples` sampled positions.
pub fn estimate(
    obs: &Observation<'_>,
    actions: &[Action],
    samples: u32,
    seed: u64,
    rollout: &BotSpec,
) -> Vec<Estimate> {
    let seat = obs.seat();
    let rules = Arc::new(obs.rules().clone());
    #[derive(Default, Clone)]
    struct Tally {
        units: Vec<f64>,
        wins: u32,
        deal_ins: u32,
    }
    let mut tallies = vec![Tally::default(); actions.len()];
    for i in 0..samples as u64 {
        let position = sample_position(obs, &mut Rng::seed(derive_seed(seed, i)));
        for (action, tally) in actions.iter().zip(&mut tallies) {
            let mut game = Game::resume(rules.clone(), position.clone());
            game.apply(seat, *action).expect("candidate actions are legal");
            let mut policies = std::array::from_fn(|k| rollout.build(derive_seed(seed ^ 0x5EED, i * 4 + k as u64)));
            play_out(&mut game, &mut policies);
            let units = game.settlement().expect("over").total()[seat.index()] as f64;
            tally.units.push(units);
            if let Some(win) = game.result().expect("over").win {
                tally.wins += (win.winner == seat) as u32;
                tally.deal_ins += (win.source == WinSource::Discard(seat)) as u32;
            }
        }
    }
    let n = samples.max(1) as f64;
    let first = tallies.first().map(|t| t.units.clone()).unwrap_or_default();
    actions
        .iter()
        .zip(&tallies)
        .map(|(&action, t)| {
            let (ev, ev_stderr) = mean_and_stderr(t.units.iter().copied());
            let (difference, difference_stderr) = mean_and_stderr(t.units.iter().zip(&first).map(|(a, b)| a - b));
            Estimate {
                action,
                samples,
                ev,
                ev_stderr,
                win_rate: t.wins as f64 / n,
                deal_in_rate: t.deal_ins as f64 / n,
                difference,
                difference_stderr,
            }
        })
        .collect()
}

fn mean_and_stderr(values: impl Iterator<Item = f64> + Clone) -> (f64, f64) {
    let n = values.clone().count().max(1) as f64;
    let mean = values.clone().sum::<f64>() / n;
    let variance = values.map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, (variance / n).sqrt())
}

#[cfg(test)]
mod tests {
    use mjers_game::wall::WALL_SIZE;
    use mjers_rules::ruleset::Ruleset;

    use super::*;

    fn midgame(seed: u64) -> Game {
        let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]);
        let mut bot = mjers_bot::heuristic::HeuristicBot::efficiency();
        for _ in 0..(20 + seed % 20) {
            let Some(&seat) = game.to_act().first() else { break };
            let action = mjers_bot::policy::decide(&game, seat, &mut bot);
            game.apply(seat, action).unwrap();
        }
        game
    }

    #[test]
    fn samples_keep_everything_the_seat_can_see() {
        for seed in 0..40 {
            let game = midgame(seed);
            let Some(&seat) = game.to_act().first() else { continue };
            let obs = game.observe(seat);
            let position = sample_position(&obs, &mut Rng::seed(seed));
            assert_eq!(position.hands[seat.index()], *game.hand(seat));
            for other in seat.others() {
                let sampled = position.hands[other.index()];
                assert_eq!(sampled.concealed().len(), game.hand(other).concealed().len());
                assert!(sampled.melds().eq(game.hand(other).melds()));
                assert_eq!(sampled.bonus(), game.hand(other).bonus());
            }
            assert_eq!(position.wall.len(), game.wall().remaining());
            let held: usize = position.hands.iter().map(|h| h.concealed().len() + h.bonus().len() as usize).sum();
            let melded_or_discarded = position.public.len();
            assert_eq!(held + melded_or_discarded + position.wall.len(), WALL_SIZE, "seed {seed}");
            let resumed = Game::resume(Arc::new(Ruleset::family()), position);
            assert_eq!(resumed.legal_actions(seat), game.legal_actions(seat));
        }
    }

    #[test]
    fn estimates_are_deterministic_and_paired() {
        let game = midgame(7);
        let seat = game.to_act()[0];
        let obs = game.observe(seat);
        let actions = obs.legal_actions();
        let rollout = BotSpec::preset("efficiency").unwrap();
        let a = estimate(&obs, &actions, 20, 1, &rollout);
        let b = estimate(&obs, &actions, 20, 1, &rollout);
        assert_eq!(a, b);
        assert_eq!(a.len(), actions.len());
        assert!(a.iter().all(|e| e.samples == 20 && e.ev_stderr >= 0.0));
    }
}
