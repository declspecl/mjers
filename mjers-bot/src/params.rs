//! The settings that distinguish one heuristic strategy from another.

use mjers_hand::shanten::MAX_SHANTEN;
use serde::{
    Deserialize,
    Serialize,
};

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
            claim_max_shanten: MAX_SHANTEN,
            caution: 0.0,
            fold_shanten: MAX_SHANTEN,
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

    /// Whether discards ever depend on danger, so threats need assessing.
    pub fn considers_danger(&self) -> bool {
        self.caution > 0.0 || self.fold_readiness <= 1.0
    }
}
