//! Instant, explainable ranking of every legal action.

use serde::{
    Deserialize,
    Serialize,
};

use crate::bot::defense::Threats;
use crate::bot::efficiency::{
    after_claim,
    after_kong,
    best_discard,
};
use crate::game::action::Action;
use crate::game::observation::{
    Decision,
    Observation,
};
use crate::hand::Hand;
use crate::hand::ukeire::Ukeire;
use crate::tile::Tile;

/// One legal action, scored and explained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assessment {
    pub action: Action,
    /// Shanten once the action (and, for chow or pong, the best discard) is done.
    pub shanten: i8,
    /// Live tiles that would then improve the hand.
    pub live_improving: u32,
    /// Rough expected number of opponents a discard deals into.
    pub danger: f64,
    /// Units collected immediately: the win, or the kong payment.
    pub immediate_units: u32,
    pub reasons: Vec<String>,
}

impl Assessment {
    /// Wins first, then lowest shanten, then paid kongs, most live tiles, least danger.
    fn sort_key(&self) -> (bool, i8, std::cmp::Reverse<u32>, std::cmp::Reverse<u32>, u64) {
        let wins = matches!(self.action, Action::Win | Action::SelfDrawWin);
        (
            !wins,
            self.shanten,
            std::cmp::Reverse(self.immediate_units),
            std::cmp::Reverse(self.live_improving),
            (self.danger * 1e6) as u64,
        )
    }
}

/// Every legal action, best first. Actions that cannot be evaluated (which
/// would mean the observation and its legal actions disagree) are left out.
pub fn rank(obs: &Observation<'_>) -> Vec<Assessment> {
    let threats = Threats::assess(obs);
    let mut assessments: Vec<Assessment> = obs.legal_actions().iter().filter_map(|&a| assess(obs, &threats, a)).collect();
    assessments.sort_by_key(Assessment::sort_key);
    assessments
}

fn assess(
    obs: &Observation<'_>,
    threats: &Threats,
    action: Action,
) -> Option<Assessment> {
    let hand = obs.hand();
    let visible = obs.public_tiles();
    let payouts = obs.rules().payouts;
    let others = 3;
    let mut reasons = Vec::new();
    let (after, extra_visible, immediate_units) = match action {
        Action::Win | Action::SelfDrawWin => {
            let units = win_units(obs, action);
            reasons.push(format!("wins now, collecting {units} units"));
            return Some(Assessment {
                action,
                shanten: -1,
                live_improving: 0,
                danger: 0.0,
                immediate_units: units,
                reasons,
            });
        },
        Action::Discard(tile) => {
            let mut after = *hand;
            after.discard(tile).ok()?;
            (after, Some(tile), 0)
        },
        Action::ConcealedKong(_) | Action::UpgradeKong(_) => {
            let after = after_kong(hand, action)?;
            let concealed = matches!(action, Action::ConcealedKong(_));
            let each = if concealed { payouts.concealed_kong } else { payouts.exposed_kong };
            reasons.push(format!("{} kong collects {} units now", if concealed { "concealed" } else { "exposed" }, each * others));
            (after, None, each * others)
        },
        Action::Kong => {
            let after = after_claim(hand, claimed_tile(obs)?, action)?;
            reasons.push(format!("exposed kong collects {} units now", payouts.exposed_kong * others));
            (after, None, payouts.exposed_kong * others)
        },
        Action::Pong | Action::Chow(_) => {
            let mut after = after_claim(hand, claimed_tile(obs)?, action)?;
            let discard = best_discard(&after, visible)?;
            after.discard(discard).ok()?;
            reasons.push(format!("then discard {discard}"));
            (after, Some(discard), 0)
        },
        Action::Pass => (*hand, None, 0),
    };
    let mut seen = *visible;
    if let Some(tile) = extra_visible {
        seen.add(tile);
    }
    let ukeire = after.ukeire(&seen);
    reasons.insert(0, describe(&ukeire, hand, &after));
    let danger = match action {
        Action::Discard(tile) => {
            let danger = threats.danger(tile);
            reasons.push(danger_reason(obs, threats, tile, danger));
            danger
        },
        _ => 0.0,
    };
    if action == Action::Pass && obs.legal_actions().contains(&Action::Kong) {
        reasons.push(format!("forgoes {} units of kong payment", payouts.exposed_kong * others));
    }
    Some(Assessment {
        action,
        shanten: ukeire.shanten,
        live_improving: ukeire.total(),
        danger,
        immediate_units,
        reasons,
    })
}

fn claimed_tile(obs: &Observation<'_>) -> Option<Tile> {
    match obs.decision() {
        Decision::Claim { tile, .. } => Some(tile),
        _ => None,
    }
}

/// What the winner would collect for this win (not counting kongs).
fn win_units(
    obs: &Observation<'_>,
    action: Action,
) -> u32 {
    let payouts = obs.rules().payouts;
    let bonus = obs.hand().bonus().len() as u32;
    let with_bonus = |base: u32| if base > 0 { base + bonus * payouts.per_bonus } else { 0 };
    match (action, obs.decision()) {
        (Action::SelfDrawWin, _) => 3 * with_bonus(payouts.self_draw_win),
        (_, Decision::Claim { from, .. }) => obs
            .seat()
            .others()
            .map(|s| with_bonus(if s == from { payouts.discard_win_discarder } else { payouts.discard_win }))
            .sum(),
        _ => 0,
    }
}

fn describe(
    ukeire: &Ukeire,
    before: &Hand,
    after: &Hand,
) -> String {
    let tiles: Vec<String> = ukeire.improvements().map(|i| format!("{}×{}", i.tile, i.live)).collect();
    let state = match ukeire.shanten {
        -1 => return "complete".to_string(),
        0 => format!("ready, waiting on {} ({} live)", tiles.join(" "), ukeire.total()),
        s => format!("{s}-shanten, {} live improving tiles: {}", ukeire.total(), tiles.join(" ")),
    };
    let melds_added = after.meld_count() > before.meld_count();
    if melds_added { format!("{state}; exposes a meld") } else { state }
}

fn danger_reason(
    obs: &Observation<'_>,
    threats: &Threats,
    tile: Tile,
    danger: f64,
) -> String {
    let safe_against: Vec<String> = threats.opponents().iter().filter(|t| t.passed.contains(tile)).map(|t| t.seat.to_string()).collect();
    let likely_ready: Vec<String> = threats
        .opponents()
        .iter()
        .filter(|t| t.readiness >= 0.5)
        .map(|t| format!("{} ({} melds)", t.seat, obs.melds(t.seat).count()))
        .collect();
    let mut reason = format!("danger {danger:.2}");
    if !safe_against.is_empty() {
        reason += &format!("; already passed by {}", safe_against.join(", "));
    }
    if !likely_ready.is_empty() {
        reason += &format!("; likely ready: {}", likely_ready.join(", "));
    }
    reason
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::game::Game;
    use crate::rules::Ruleset;
    use crate::rules::seat::Seat;

    #[test]
    fn ranks_every_legal_action_with_reasons() {
        let rules = Arc::new(Ruleset::family());
        for seed in 0..30 {
            let game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
            let obs = game.observe(Seat::ALL[0]);
            let ranked = rank(&obs);
            assert_eq!(ranked.len(), obs.legal_actions().len());
            assert!(ranked.iter().all(|a| !a.reasons.is_empty()));
            assert!(ranked.windows(2).all(|w| w[0].sort_key() <= w[1].sort_key()));
        }
    }

    /// Plays efficiency bots until some seat may declare a self-drawn win.
    fn game_with_self_draw_offer() -> Option<(Game, Seat)> {
        let rules = Arc::new(Ruleset::family());
        for seed in 0..1_000 {
            let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
            let mut bot = crate::bot::heuristic::HeuristicBot::efficiency();
            while let Some(seat) = game.to_act().first() {
                if game.legal_actions(seat).contains(&Action::SelfDrawWin) {
                    return Some((game, seat));
                }
                let action = crate::bot::policy::decide(&game, seat, &mut bot);
                game.apply(seat, action).unwrap();
            }
        }
        None
    }

    #[test]
    fn wins_rank_first_and_report_their_value() {
        let (game, seat) = game_with_self_draw_offer().expect("some hand offers a self-drawn win");
        let ranked = rank(&game.observe(seat));
        assert_eq!(ranked[0].action, Action::SelfDrawWin);
        let bonus = game.hand(seat).bonus().len() as u32;
        assert_eq!(ranked[0].immediate_units, 3 * (3 + bonus));
        assert!(ranked[0].reasons[0].starts_with("wins now"));
    }
}
