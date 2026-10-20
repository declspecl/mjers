//! The heuristic bot: efficiency-first play shaped by `Params`.

#[cfg(test)]
mod tests;

use crate::bot::defense::Threats;
use crate::bot::efficiency::{
    after_claim,
    after_kong,
    best_discard_shanten,
    connectivity,
};
use crate::bot::params::{
    ClaimRule,
    KongRule,
    Params,
};
use crate::bot::policy::Policy;
use crate::game::action::Action;
use crate::game::observation::{
    Decision,
    Observation,
};
use crate::hand::Hand;
use crate::tile::Tile;

pub struct HeuristicBot {
    name: String,
    params: Params,
}

impl HeuristicBot {
    pub fn new(
        name: impl Into<String>,
        params: Params,
    ) -> HeuristicBot {
        HeuristicBot { name: name.into(), params }
    }

    pub fn efficiency() -> HeuristicBot {
        HeuristicBot::new("efficiency", Params::efficiency())
    }

    pub fn defensive() -> HeuristicBot {
        HeuristicBot::new("defensive", Params::defensive())
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    fn turn(
        &self,
        obs: &Observation<'_>,
        legal: &[Action],
    ) -> Option<Action> {
        let hand = obs.hand();
        // Only computed if some kong needs comparing against the best discard.
        let mut shanten_after_discard = None;
        for &action in legal {
            let rule = match action {
                Action::ConcealedKong(_) => self.params.concealed_kong,
                Action::UpgradeKong(_) => self.params.upgrade_kong,
                _ => continue,
            };
            let declare = match rule {
                KongRule::Never => false,
                KongRule::Always => true,
                KongRule::WhenFree => {
                    let current = *shanten_after_discard.get_or_insert_with(|| best_discard_shanten(hand));
                    after_kong(hand, action).is_some_and(|h| h.shanten() <= current)
                },
            };
            if declare {
                return Some(action);
            }
        }
        self.discard(obs).map(Action::Discard)
    }

    /// The best discard by efficiency, weighed against danger when the
    /// strategy cares about it.
    fn discard(
        &self,
        obs: &Observation<'_>,
    ) -> Option<Tile> {
        let hand = obs.hand();
        let options = hand.discard_options(obs.public_tiles());
        let best = options.first()?.ukeire;
        let threats = self.params.considers_danger().then(|| Threats::assess(obs));
        let folding = threats
            .as_ref()
            .is_some_and(|t| best.shanten >= self.params.fold_shanten && t.max_readiness() >= self.params.fold_readiness);
        let score = |tile: Tile, shanten: i8, live: u32| {
            let danger = threats.as_ref().map_or(0.0, |t| t.danger(tile));
            if folding {
                return danger;
            }
            let slower = (shanten - best.shanten) as f64;
            let fewer = (best.total() as f64 - live as f64) / best.total().max(1) as f64;
            slower + 0.3 * fewer + self.params.caution * danger
        };
        options
            .iter()
            .map(|o| {
                (
                    score(o.tile, o.ukeire.shanten, o.ukeire.total()),
                    connectivity(hand.concealed(), o.tile),
                    std::cmp::Reverse(o.tile.is_honor()),
                    o.tile,
                )
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| (a.1, a.2, a.3).cmp(&(b.1, b.2, b.3))))
            .map(|(_, _, _, tile)| tile)
    }

    /// Takes the permitted claim that leaves the lowest shanten, otherwise passes.
    pub fn claim(
        &self,
        hand: &Hand,
        tile: Tile,
        legal: &[Action],
    ) -> Action {
        let current = hand.shanten();
        let mut best: Option<(i8, Action)> = None;
        for &action in legal {
            let Some(after) = after_claim(hand, tile, action) else {
                continue;
            };
            let accepted = match action {
                Action::Kong => {
                    let score = after.shanten();
                    let take = match self.params.exposed_kong {
                        KongRule::Never => false,
                        KongRule::Always => true,
                        KongRule::WhenFree => score <= current,
                    };
                    take.then_some(score)
                },
                _ => {
                    let rule = if action == Action::Pong { self.params.pong } else { self.params.chow };
                    let score = best_discard_shanten(&after);
                    let allowed = current <= self.params.claim_max_shanten;
                    let take = match rule {
                        ClaimRule::Never => false,
                        ClaimRule::Always => true,
                        ClaimRule::WhenImproves => score < current,
                    };
                    (allowed && take).then_some(score)
                },
            };
            // Kongs win ties: they are paid.
            if let Some(score) = accepted
                && best.is_none_or(|(s, a)| score < s || (score == s && action == Action::Kong && a != Action::Kong))
            {
                best = Some((score, action));
            }
        }
        best.map_or(Action::Pass, |(_, action)| action)
    }
}

impl Policy for HeuristicBot {
    fn name(&self) -> &str {
        &self.name
    }

    fn act(
        &mut self,
        obs: &Observation<'_>,
        legal: &[Action],
    ) -> Action {
        for win in [Action::Win, Action::SelfDrawWin] {
            if legal.contains(&win) {
                return win;
            }
        }
        let chosen = match obs.decision() {
            Decision::Turn { .. } => self.turn(obs, legal),
            Decision::Claim { tile, .. } => Some(self.claim(obs.hand(), tile, legal)),
            Decision::None => None,
        };
        chosen.or_else(|| legal.first().copied()).unwrap_or(Action::Pass)
    }
}
