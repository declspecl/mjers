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
use crate::tile::counts::TileCounts;

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
    let mut seen = *obs.public_tiles();
    let payouts = obs.rules().payouts;
    let others = 3;
    let mut reasons = Vec::new();
    let (after, immediate_units) = match action {
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
            seen.add(tile);
            (after, 0)
        },
        Action::ConcealedKong(_) | Action::UpgradeKong(_) => {
            let after = after_kong(hand, action)?;
            reveal_meld_tiles(hand, &after, &mut seen);
            let concealed = matches!(action, Action::ConcealedKong(_));
            let each = if concealed { payouts.concealed_kong } else { payouts.exposed_kong };
            reasons.push(format!("{} kong collects {} units now", if concealed { "concealed" } else { "exposed" }, each * others));
            (after, each * others)
        },
        Action::Kong => {
            let after = after_claim(hand, claimed_tile(obs)?, action)?;
            reveal_meld_tiles(hand, &after, &mut seen);
            reasons.push(format!("exposed kong collects {} units now", payouts.exposed_kong * others));
            (after, payouts.exposed_kong * others)
        },
        Action::Pong | Action::Chow(_) => {
            let mut after = after_claim(hand, claimed_tile(obs)?, action)?;
            reveal_meld_tiles(hand, &after, &mut seen);
            let discard = best_discard(&after, &seen)?;
            after.discard(discard).ok()?;
            seen.add(discard);
            reasons.push(format!("then discard {discard}"));
            (after, 0)
        },
        Action::Pass => (*hand, 0),
    };
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

/// The offered tile and any existing pong are already public. Only copies
/// leaving the concealed hand become newly visible when declaring a meld.
fn reveal_meld_tiles(
    before: &Hand,
    after: &Hand,
    visible: &mut TileCounts,
) {
    for tile in before.concealed().kinds() {
        let revealed = before.concealed().get(tile).saturating_sub(after.concealed().get(tile));
        for _ in 0..revealed {
            visible.add(tile);
        }
    }
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
    use crate::bot::policy::{
        Policy,
        RandomBot,
    };
    use crate::game::Game;
    use crate::game::action::Phase;
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

    #[test]
    fn meld_assessments_match_tiles_published_by_the_engine() {
        let rules = Arc::new(Ruleset::family());
        let mut covered = [false; 5];
        let mut bot = RandomBot::new(17);
        for seed in 0..1_000 {
            let mut game = Game::new(rules.clone(), seed, Seat::ALL[0]).unwrap();
            while let Some(seat) = game.to_act().first() {
                let obs = game.observe(seat);
                let legal = obs.legal_actions();
                for &action in legal.iter() {
                    let category = match action {
                        Action::Chow(_) => 0,
                        Action::Pong => 1,
                        Action::Kong => 2,
                        Action::ConcealedKong(_) => 3,
                        Action::UpgradeKong(_) => 4,
                        _ => continue,
                    };
                    if covered[category] {
                        continue;
                    }
                    let mut branch = game.clone();
                    branch.apply(seat, action).unwrap();
                    while matches!(branch.phase(), Phase::Claim { .. }) {
                        let other = branch.to_act().first().unwrap();
                        branch.apply(other, Action::Pass).unwrap();
                    }
                    if branch.is_over() {
                        continue;
                    }
                    let mut stale_visible = *obs.public_tiles();
                    let discard = if matches!(action, Action::Chow(_) | Action::Pong) {
                        let tile = best_discard(branch.hand(seat), branch.public_tiles()).unwrap();
                        branch.apply(seat, Action::Discard(tile)).unwrap();
                        stale_visible.add(tile);
                        Some(tile)
                    } else {
                        None
                    };
                    let mut after = *branch.hand(seat);
                    if discard.is_none() {
                        // Kongs draw immediately. Assessments describe the
                        // waiting hand before that unknown replacement tile.
                        after.discard(branch.last_draw().unwrap()).unwrap();
                    }
                    let expected = after.ukeire(branch.public_tiles());
                    // Require a real availability difference for every meld
                    // kind, rather than cases where revealed tiles cannot help.
                    if expected.total() == after.ukeire(&stale_visible).total() {
                        continue;
                    }
                    let assessment = assess(&obs, &Threats::assess(&obs), action).unwrap();
                    assert_eq!(assessment.shanten, expected.shanten, "seed {seed}, {action:?}");
                    assert_eq!(assessment.live_improving, expected.total(), "seed {seed}, {action:?}");
                    if let Some(tile) = discard {
                        assert!(assessment.reasons.contains(&format!("then discard {tile}")), "seed {seed}, {action:?}");
                    }
                    covered[category] = true;
                }
                if covered.iter().all(|&seen| seen) {
                    return;
                }
                let action = bot.act(&obs, &legal);
                game.apply(seat, action).unwrap();
            }
        }
        assert_eq!(covered, [true; 5], "exercise every kind of meld");
    }
}
