//! Hand-efficiency play: get to ready fast, then win.

#[cfg(test)]
mod tests;

use mjers_game::{
    Action,
    Decision,
    Observation,
};
use mjers_hand::{
    DiscardOption,
    Hand,
    shanten,
};
use mjers_tile::tile::SUITED_KINDS;
use mjers_tile::{
    Tile,
    TileCounts,
};

use crate::defense;
use crate::policy::Policy;

/// Plays for speed: minimise shanten, then maximise live improving tiles.
pub struct EfficiencyBot;

impl Policy for EfficiencyBot {
    fn name(&self) -> &str {
        "efficiency"
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
        let hand = obs.hand();
        let visible = obs.public_tiles();
        match obs.decision() {
            Decision::Turn { .. } => {
                let current = best_discard_shanten(hand);
                let kong = legal.iter().copied().find(|&a| after_kong(hand, a).is_some_and(|h| h.shanten() <= current));
                kong.unwrap_or_else(|| Action::Discard(best_discard(hand, visible)))
            },
            Decision::Claim { tile, .. } => best_claim(hand, tile, legal),
            Decision::None => unreachable!("asked to act without a decision"),
        }
    }
}

/// The hand after a turn-time kong declaration, before its replacement draw.
pub(crate) fn after_kong(
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
pub(crate) fn best_discard_shanten(hand: &Hand) -> i8 {
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

/// Lowest shanten, then most live improving tiles, then least connected.
pub(crate) fn best_discard(
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

/// How many held tiles could combine with `tile`: copies and suit neighbours
/// within two ranks.
pub(crate) fn connectivity(
    concealed: &TileCounts,
    tile: Tile,
) -> u8 {
    let i = tile.index();
    let copies = concealed.get(tile) - 1;
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

/// Takes the claim that most lowers shanten (kongs only when they cost
/// nothing), otherwise passes.
fn best_claim(
    hand: &Hand,
    tile: Tile,
    legal: &[Action],
) -> Action {
    let current = hand.shanten();
    let mut best = (current, Action::Pass);
    for &action in legal {
        let Some(after) = after_claim(hand, tile, action) else {
            continue;
        };
        let score = if action == Action::Kong { after.shanten() } else { best_discard_shanten(&after) };
        let good_enough = if action == Action::Kong { score <= current } else { score < current };
        if good_enough && score <= best.0 {
            best = (score, action);
        }
    }
    best.1
}

/// The hand right after claiming `tile` (before discarding or replacing).
pub(crate) fn after_claim(
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

/// Efficiency play that weighs each discard's danger, and folds (discards
/// the safest tile) when far from ready while an opponent looks ready.
pub struct DefensiveBot {
    /// Weight of danger against lost efficiency.
    pub caution: f64,
    /// Fold at this shanten or worse...
    pub fold_shanten: i8,
    /// ...when some opponent's readiness reaches this.
    pub fold_readiness: f64,
}

impl Default for DefensiveBot {
    fn default() -> Self {
        DefensiveBot {
            caution: 1.0,
            fold_shanten: 2,
            fold_readiness: 0.5,
        }
    }
}

impl Policy for DefensiveBot {
    fn name(&self) -> &str {
        "defensive"
    }

    fn act(
        &mut self,
        obs: &Observation<'_>,
        legal: &[Action],
    ) -> Action {
        let efficient = EfficiencyBot.act(obs, legal);
        let Action::Discard(_) = efficient else {
            return efficient;
        };
        Action::Discard(self.safest_reasonable_discard(obs))
    }
}

impl DefensiveBot {
    fn safest_reasonable_discard(
        &self,
        obs: &Observation<'_>,
    ) -> Tile {
        let hand = obs.hand();
        let options = hand.discard_options(obs.public_tiles());
        let unseen = defense::unseen(obs);
        let threat = obs.seat().others().map(|o| defense::readiness(obs, o)).fold(0.0, f64::max);
        let best = &options[0].ukeire;
        let folding = best.shanten >= self.fold_shanten && threat >= self.fold_readiness;
        let score = |o: &DiscardOption| {
            let danger = defense::danger(obs, o.tile, &unseen);
            if folding {
                return danger;
            }
            let slower = (o.ukeire.shanten - best.shanten) as f64;
            let fewer = (best.total() as f64 - o.ukeire.total() as f64) / best.total().max(1) as f64;
            slower + 0.3 * fewer + self.caution * danger
        };
        options.iter().min_by(|a, b| score(a).total_cmp(&score(b))).expect("a hand awaiting discard holds tiles").tile
    }
}
