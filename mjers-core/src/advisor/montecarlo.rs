//! Estimating each action's value by playing it out on sampled deals.
//!
//! Hidden information (opponents' concealed tiles and the wall order) is
//! resampled consistently with what the seat can see, each candidate action is
//! applied, and bots play the hand out. Every candidate sees the same samples,
//! so differences between them are not luck of the sample.

use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};

use crate::bot::policy::play_out;
use crate::bot::spec::BotSpec;
use crate::game::Game;
use crate::game::action::{
    Action,
    Offer,
};
use crate::game::observation::{
    Decision,
    Observation,
};
use crate::game::position::{
    Position,
    ResumePoint,
};
use crate::game::rng::{
    Rng,
    derive_seed,
};
use crate::game::wall::WallTile;
use crate::hand::Hand;
use crate::rules::Ruleset;
use crate::rules::seat::{
    SEATS,
    Seat,
};
use crate::rules::settlement::WinSource;
use crate::tile::counts::{
    COPIES,
    TileCounts,
};
use crate::tile::{
    Bonus,
    Tile,
};

/// One way the hidden tiles could lie, given what `obs` shows. `None` if the
/// observation has no decision or its tile counts cannot be satisfied.
pub fn sample_position(
    obs: &Observation<'_>,
    rng: &mut Rng,
) -> Option<Position> {
    let seat = obs.seat();
    let point = match obs.decision() {
        Decision::Turn { after_claim, drawn } => ResumePoint::Turn { seat, after_claim, last_draw: drawn },
        Decision::Claim { tile, from, offer } => ResumePoint::Claim { tile, from, offer },
        Decision::None => return None,
    };
    let own = obs.hand().concealed();
    let public = obs.public_tiles();
    let mut hidden: Vec<Tile> = Tile::all().flat_map(|t| std::iter::repeat_n(t, COPIES.saturating_sub(own.get(t) + public.get(t)) as usize)).collect();
    rng.shuffle(&mut hidden);

    let mut concealed_by_seat: [TileCounts; SEATS] = Default::default();
    concealed_by_seat[seat.index()] = *own;
    // A player adding to a kong that may be robbed must hold the tile.
    if let Decision::Claim {
        tile,
        from,
        offer: Offer::KongUpgrade,
    } = obs.decision()
    {
        let i = hidden.iter().position(|&t| t == tile)?;
        hidden.swap_remove(i);
        concealed_by_seat[from.index()].add(tile);
    }
    for other in seat.others() {
        while concealed_by_seat[other.index()].len() < obs.concealed_count(other) {
            concealed_by_seat[other.index()].add(hidden.pop()?);
        }
    }
    let held_bonuses: Vec<Bonus> = Seat::ALL.iter().flat_map(|&s| obs.bonus(s).iter()).collect();
    let unseen_bonuses = Bonus::all().filter(|b| !held_bonuses.contains(b)).map(WallTile::Bonus);
    let mut wall: Vec<WallTile> = hidden.into_iter().map(WallTile::Tile).chain(unseen_bonuses).collect();
    rng.shuffle(&mut wall);
    if wall.len() != obs.wall_remaining() {
        return None;
    }

    let mut hands = [Hand::new(TileCounts::new()); SEATS];
    for s in Seat::ALL {
        hands[s.index()] = Hand::from_parts(concealed_by_seat[s.index()], obs.melds(s), obs.bonus(s)).ok()?;
    }
    Some(Position {
        hands,
        wall,
        dealer: obs.dealer(),
        discards: obs.discards().to_vec(),
        public: *public,
        point,
    })
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

/// Plays each of `actions` out from up to `samples` sampled positions.
/// A sample that cannot be played for every action is skipped for all of
/// them, so the estimates stay paired.
pub fn estimate(
    obs: &Observation<'_>,
    actions: &[Action],
    samples: u32,
    seed: u64,
    rollout: &BotSpec,
) -> Vec<Estimate> {
    let seat = obs.seat();
    let rules = Arc::new(obs.rules().clone());
    let mut outcomes_by_action: Vec<Vec<Outcome>> = vec![Vec::with_capacity(samples as usize); actions.len()];
    for i in 0..samples as u64 {
        let Some(position) = sample_position(obs, &mut Rng::seed(derive_seed(seed, i))) else {
            continue;
        };
        let played: Option<Vec<Outcome>> = actions
            .iter()
            .map(|&action| play_from(&rules, &position, seat, action, rollout, derive_seed(seed ^ 0x5EED, i)))
            .collect();
        if let Some(played) = played {
            for (outcomes, outcome) in outcomes_by_action.iter_mut().zip(played) {
                outcomes.push(outcome);
            }
        }
    }
    let first = outcomes_by_action.first().cloned().unwrap_or_default();
    actions
        .iter()
        .zip(&outcomes_by_action)
        .map(|(&action, outcomes)| {
            let n = outcomes.len().max(1) as f64;
            let (ev, ev_stderr) = mean_and_stderr(outcomes.iter().map(|o| o.units));
            let (difference, difference_stderr) = mean_and_stderr(outcomes.iter().zip(&first).map(|(a, b)| a.units - b.units));
            Estimate {
                action,
                samples: outcomes.len() as u32,
                ev,
                ev_stderr,
                win_rate: outcomes.iter().filter(|o| o.won).count() as f64 / n,
                deal_in_rate: outcomes.iter().filter(|o| o.dealt_in).count() as f64 / n,
                difference,
                difference_stderr,
            }
        })
        .collect()
}

/// How one playout ended for the advised seat.
#[derive(Debug, Clone, Copy)]
struct Outcome {
    units: f64,
    won: bool,
    dealt_in: bool,
}

fn play_from(
    rules: &Arc<Ruleset>,
    position: &Position,
    seat: Seat,
    action: Action,
    rollout: &BotSpec,
    seed: u64,
) -> Option<Outcome> {
    let mut game = Game::resume(rules.clone(), position.clone()).ok()?;
    game.apply(seat, action).ok()?;
    let mut policies = std::array::from_fn(|k| rollout.build(derive_seed(seed, k as u64)));
    play_out(&mut game, &mut policies).ok()?;
    let units = game.settlement()?.total()[seat.index()] as f64;
    let win = game.result()?.win;
    Some(Outcome {
        units,
        won: win.is_some_and(|w| w.winner == seat),
        dealt_in: win.is_some_and(|w| w.source == WinSource::Discard(seat)),
    })
}

fn mean_and_stderr(values: impl Iterator<Item = f64> + Clone) -> (f64, f64) {
    let n = values.clone().count().max(1) as f64;
    let mean = values.clone().sum::<f64>() / n;
    let variance = values.map(|v| (v - mean) * (v - mean)).sum::<f64>() / (n - 1.0).max(1.0);
    (mean, (variance / n).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::wall::WALL_SIZE;
    use crate::rules::Ruleset;

    fn midgame(seed: u64) -> Game {
        let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]).unwrap();
        let mut bot = crate::bot::heuristic::HeuristicBot::efficiency();
        for _ in 0..(20 + seed % 20) {
            let Some(seat) = game.to_act().first() else { break };
            let action = crate::bot::policy::decide(&game, seat, &mut bot);
            game.apply(seat, action).unwrap();
        }
        game
    }

    #[test]
    fn samples_keep_everything_the_seat_can_see() {
        for seed in 0..40 {
            let game = midgame(seed);
            let Some(seat) = game.to_act().first() else { continue };
            let obs = game.observe(seat);
            let position = sample_position(&obs, &mut Rng::seed(seed)).unwrap();
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
            let resumed = Game::resume(Arc::new(Ruleset::family()), position).unwrap();
            assert_eq!(resumed.legal_actions(seat), game.legal_actions(seat));
        }
    }

    #[test]
    fn estimates_are_deterministic_and_paired() {
        let game = midgame(7);
        let seat = game.to_act().first().unwrap();
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
