//! Configurable hand-efficiency play with optional defence.

#[cfg(test)]
mod tests;

use mjers_game::action::Action;
use mjers_game::observation::{
    Decision,
    Observation,
};
use mjers_hand::hand::Hand;
use mjers_hand::shanten;
use mjers_hand::ukeire::DiscardOption;
use mjers_tile::counts::TileCounts;
use mjers_tile::tile::{
    SUITED_KINDS,
    Tile,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::defense;
use crate::policy::Policy;

/// When to chow or pong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimRule {
    Never,
    /// Only when the claim (and the best discard after it) lowers shanten.
    WhenImproves,
    /// Whenever the claim is legal.
    Always,
}

/// When to declare a kong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KongRule {
    Never,
    /// Only when it does not raise shanten.
    WhenFree,
    Always,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Params {
    pub chow: ClaimRule,
    pub pong: ClaimRule,
    /// Kong claimed from a discard.
    pub exposed_kong: KongRule,
    pub concealed_kong: KongRule,
    pub upgrade_kong: KongRule,
    /// Chow and pong only at this shanten or better.
    pub claim_max_shanten: i8,
    /// Weight of a discard's danger against lost efficiency; 0 ignores danger.
    pub caution: f64,
    /// Fold (discard the safest tile) at this shanten or worse...
    pub fold_shanten: i8,
    /// ...once some opponent's estimated readiness reaches this. Above 1 never folds.
    pub fold_readiness: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params::efficiency()
    }
}

impl Params {
    /// Pure speed: claim when it helps, kong when free, ignore danger.
    pub fn efficiency() -> Params {
        Params {
            chow: ClaimRule::WhenImproves,
            pong: ClaimRule::WhenImproves,
            exposed_kong: KongRule::WhenFree,
            concealed_kong: KongRule::WhenFree,
            upgrade_kong: KongRule::WhenFree,
            claim_max_shanten: shanten::MAX_SHANTEN,
            caution: 0.0,
            fold_shanten: shanten::MAX_SHANTEN,
            fold_readiness: 2.0,
        }
    }

    /// Efficiency that weighs danger and folds hopeless hands.
    pub fn defensive() -> Params {
        Params {
            caution: 1.0,
            fold_shanten: 2,
            fold_readiness: 0.5,
            ..Params::efficiency()
        }
    }
}

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
    ) -> Action {
        let hand = obs.hand();
        let current = best_discard_shanten(hand);
        let kong = legal.iter().copied().find(|&action| {
            let rule = match action {
                Action::ConcealedKong(_) => self.params.concealed_kong,
                Action::UpgradeKong(_) => self.params.upgrade_kong,
                _ => return false,
            };
            match rule {
                KongRule::Never => false,
                KongRule::Always => true,
                KongRule::WhenFree => after_kong(hand, action).is_some_and(|h| h.shanten() <= current),
            }
        });
        kong.unwrap_or_else(|| Action::Discard(self.discard(obs)))
    }

    fn discard(
        &self,
        obs: &Observation<'_>,
    ) -> Tile {
        let hand = obs.hand();
        let options = hand.discard_options(obs.public_tiles());
        let best = &options[0].ukeire;
        let unseen = defense::unseen(obs);
        let threat = obs.seat().others().map(|o| defense::readiness(obs, o)).fold(0.0, f64::max);
        let folding = best.shanten >= self.params.fold_shanten && threat >= self.params.fold_readiness;
        let score = |o: &DiscardOption| {
            let danger = if folding || self.params.caution > 0.0 { defense::danger(obs, o.tile, &unseen) } else { 0.0 };
            if folding {
                return danger;
            }
            let slower = (o.ukeire.shanten - best.shanten) as f64;
            let fewer = (best.total() as f64 - o.ukeire.total() as f64) / best.total().max(1) as f64;
            slower + 0.3 * fewer + self.params.caution * danger
        };
        options
            .iter()
            .map(|o| (score(o), connectivity(hand.concealed(), o.tile), std::cmp::Reverse(o.tile.is_honor()), o.tile))
            .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| (a.1, a.2, a.3).cmp(&(b.1, b.2, b.3))))
            .expect("a hand awaiting discard holds tiles")
            .3
    }

    /// Takes the permitted claim that leaves the lowest shanten, otherwise passes.
    fn claim(
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
                    let ok = match self.params.exposed_kong {
                        KongRule::Never => false,
                        KongRule::Always => true,
                        KongRule::WhenFree => score <= current,
                    };
                    ok.then_some(score)
                },
                _ => {
                    let rule = if action == Action::Pong { self.params.pong } else { self.params.chow };
                    let score = best_discard_shanten(&after);
                    let ok = current <= self.params.claim_max_shanten
                        && match rule {
                            ClaimRule::Never => false,
                            ClaimRule::Always => true,
                            ClaimRule::WhenImproves => score < current,
                        };
                    ok.then_some(score)
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
        match obs.decision() {
            Decision::Turn { .. } => self.turn(obs, legal),
            Decision::Claim { tile, .. } => self.claim(obs.hand(), tile, legal),
            Decision::None => unreachable!("asked to act without a decision"),
        }
    }
}

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

/// Shanten after the best discard from a hand awaiting discard.
pub fn best_discard_shanten(hand: &Hand) -> i8 {
    hand.concealed()
        .kinds()
        .map(|t| {
            let mut after = *hand;
            after.discard(t).expect("held");
            after.shanten()
        })
        .min()
        .unwrap_or(shanten::MAX_SHANTEN)
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

/// The fastest discard ignoring danger: lowest shanten, most live improving
/// tiles, then least connected.
pub fn best_discard(
    hand: &Hand,
    visible: &TileCounts,
) -> Tile {
    let options = hand.discard_options(visible);
    let best = &options[0].ukeire;
    options
        .iter()
        .filter(|o| o.ukeire.shanten == best.shanten && o.ukeire.total() == best.total())
        .map(|o| o.tile)
        .min_by_key(|&t| (connectivity(hand.concealed(), t), std::cmp::Reverse(t.is_honor()), t))
        .expect("a hand awaiting discard holds tiles")
}
