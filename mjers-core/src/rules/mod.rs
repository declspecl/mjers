//! Rule variants as data, so experiments can compare them.

pub mod seat;
pub mod settlement;

#[cfg(test)]
mod tests;

use serde::{
    Deserialize,
    Serialize,
};

/// Payment amounts in units, each paid separately by every listed payer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payouts {
    /// Paid by each non-winner other than the discarder on a discard win.
    pub discard_win: u32,
    /// Paid by the discarder on a discard win.
    pub discard_win_discarder: u32,
    /// Paid by each non-winner on a self-drawn win.
    pub self_draw_win: u32,
    /// Paid per winner's flower or season, by everyone paying for the win.
    pub per_bonus: u32,
    /// Paid to the owner of an exposed kong by each other player.
    pub exposed_kong: u32,
    /// Paid to the owner of a concealed kong by each other player.
    pub concealed_kong: u32,
    /// Whether kongs are paid when the wall runs out with no winner.
    pub kongs_paid_without_winner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ruleset {
    pub name: String,
    /// Stacks of two kept back for replacement draws; normal draws stop when
    /// only these remain.
    pub flower_wall_stacks: u8,
    /// Whether a player may win with the tile another adds to an exposed pong.
    pub allow_rob_kong: bool,
    pub payouts: Payouts,
}

impl Ruleset {
    pub const PRESETS: [&str; 2] = ["family", "discarder-pays"];

    /// The family rules in `RULES.md`.
    pub fn family() -> Self {
        Ruleset {
            name: "family".into(),
            flower_wall_stacks: 8,
            allow_rob_kong: false,
            payouts: Payouts {
                discard_win: 1,
                discard_win_discarder: 2,
                self_draw_win: 3,
                per_bonus: 1,
                exposed_kong: 1,
                concealed_kong: 2,
                kongs_paid_without_winner: true,
            },
        }
    }

    /// Family rules, except only the discarder pays for a discard win, covering
    /// what all three players would have paid.
    pub fn discarder_pays() -> Self {
        let family = Self::family();
        Ruleset {
            name: "discarder-pays".into(),
            payouts: Payouts {
                discard_win: 0,
                discard_win_discarder: 4,
                ..family.payouts
            },
            ..family
        }
    }

    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "family" => Some(Self::family()),
            "discarder-pays" => Some(Self::discarder_pays()),
            _ => None,
        }
    }

    /// Tiles held back from normal draws.
    pub const fn reserved_tiles(&self) -> usize {
        self.flower_wall_stacks as usize * 2
    }

    /// Checks the settings can be played: the flower wall must leave enough
    /// tiles to deal every hand and give the dealer a first draw.
    pub fn validate(&self) -> Result<(), RulesError> {
        if self.reserved_tiles() > MAX_RESERVED_TILES {
            return Err(RulesError::FlowerWallTooLarge { stacks: self.flower_wall_stacks });
        }
        Ok(())
    }
}

/// 144 tiles less four 13-tile hands and the dealer's first draw.
const MAX_RESERVED_TILES: usize = 144 - 4 * 13 - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesError {
    FlowerWallTooLarge { stacks: u8 },
}

impl std::fmt::Display for RulesError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            RulesError::FlowerWallTooLarge { stacks } => {
                write!(f, "a flower wall of {stacks} stacks leaves too few tiles to deal (at most {} stacks)", MAX_RESERVED_TILES / 2)
            },
        }
    }
}

impl std::error::Error for RulesError {}

impl Default for Ruleset {
    fn default() -> Self {
        Self::family()
    }
}
