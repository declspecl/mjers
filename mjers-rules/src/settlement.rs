//! End-of-hand payments.

#[cfg(test)]
mod tests;

use mjers_hand::meld::KongKind;
use serde::{
    Deserialize,
    Serialize,
};

use crate::ruleset::Payouts;
use crate::seat::{
    SEATS,
    Seat,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WinSource {
    SelfDraw,
    /// Won off this seat's discard (or, with robbing allowed, its kong tile).
    Discard(Seat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Win {
    pub winner: Seat,
    pub source: WinSource,
    /// Flowers and seasons the winner set aside.
    pub bonus_tiles: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kong {
    pub owner: Seat,
    pub kind: KongKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandResult {
    /// `None` when the wall ran out.
    pub win: Option<Win>,
    pub kongs: Vec<Kong>,
}

/// Per-seat unit changes, split by cause. Each part sums to zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Settlement {
    pub win: [i32; SEATS],
    pub kongs: [i32; SEATS],
}

impl Settlement {
    pub fn total(&self) -> [i32; SEATS] {
        std::array::from_fn(|i| self.win[i] + self.kongs[i])
    }
}

impl Payouts {
    pub fn settle(
        &self,
        result: &HandResult,
    ) -> Settlement {
        let mut settlement = Settlement::default();
        if let Some(win) = result.win {
            for payer in win.winner.others() {
                let base = match win.source {
                    WinSource::SelfDraw => self.self_draw_win,
                    WinSource::Discard(discarder) if discarder == payer => self.discard_win_discarder,
                    WinSource::Discard(_) => self.discard_win,
                };
                let paid_for_win = base > 0;
                let amount = base + if paid_for_win { self.per_bonus * win.bonus_tiles as u32 } else { 0 };
                transfer(&mut settlement.win, payer, win.winner, amount);
            }
        }
        if result.win.is_some() || self.kongs_paid_without_winner {
            for kong in &result.kongs {
                let amount = match kong.kind {
                    KongKind::Exposed => self.exposed_kong,
                    KongKind::Concealed => self.concealed_kong,
                };
                for payer in kong.owner.others() {
                    transfer(&mut settlement.kongs, payer, kong.owner, amount);
                }
            }
        }
        settlement
    }
}

fn transfer(
    deltas: &mut [i32; SEATS],
    from: Seat,
    to: Seat,
    amount: u32,
) {
    deltas[from.index()] -= amount as i32;
    deltas[to.index()] += amount as i32;
}
