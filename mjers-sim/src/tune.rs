//! Coordinate search over heuristic bot parameters.
//!
//! Each step tries every alternative value of one parameter against the
//! current champion on the training seeds (duplicate comparison against a
//! fixed field) and adopts the best if it wins by `min_z` standard errors.
//! Afterwards the champion is compared with the starting point on the next,
//! disjoint seed block, which never influences selection.

use mjers_bot::{
    BotSpec,
    ClaimRule,
    KongRule,
    Params,
};
use mjers_game::Ruleset;
use serde::{
    Deserialize,
    Serialize,
};

use crate::compare::compare;
use crate::report::ComparisonResult;
use crate::runner::Seeds;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Chow,
    Pong,
    ExposedKong,
    ConcealedKong,
    UpgradeKong,
    ClaimMaxShanten,
    Caution,
    FoldShanten,
    FoldReadiness,
}

impl Dimension {
    pub const ALL: [Dimension; 9] = [
        Dimension::Chow,
        Dimension::Pong,
        Dimension::ExposedKong,
        Dimension::ConcealedKong,
        Dimension::UpgradeKong,
        Dimension::ClaimMaxShanten,
        Dimension::Caution,
        Dimension::FoldShanten,
        Dimension::FoldReadiness,
    ];

    /// Every value this dimension can take, applied to `base`.
    pub fn variants(
        self,
        base: &Params,
    ) -> Vec<Params> {
        const CLAIMS: [ClaimRule; 3] = [ClaimRule::Never, ClaimRule::WhenImproves, ClaimRule::Always];
        const KONGS: [KongRule; 3] = [KongRule::Never, KongRule::WhenFree, KongRule::Always];
        let with = |f: &dyn Fn(&mut Params)| {
            let mut p = base.clone();
            f(&mut p);
            p
        };
        match self {
            Dimension::Chow => CLAIMS.iter().map(|&r| with(&|p| p.chow = r)).collect(),
            Dimension::Pong => CLAIMS.iter().map(|&r| with(&|p| p.pong = r)).collect(),
            Dimension::ExposedKong => KONGS.iter().map(|&r| with(&|p| p.exposed_kong = r)).collect(),
            Dimension::ConcealedKong => KONGS.iter().map(|&r| with(&|p| p.concealed_kong = r)).collect(),
            Dimension::UpgradeKong => KONGS.iter().map(|&r| with(&|p| p.upgrade_kong = r)).collect(),
            Dimension::ClaimMaxShanten => [0, 1, 2, 3, 8].iter().map(|&s| with(&|p| p.claim_max_shanten = s)).collect(),
            Dimension::Caution => [0.0, 0.25, 0.5, 1.0, 2.0].iter().map(|&c| with(&|p| p.caution = c)).collect(),
            Dimension::FoldShanten => [1, 2, 3, 8].iter().map(|&s| with(&|p| p.fold_shanten = s)).collect(),
            Dimension::FoldReadiness => [0.3, 0.5, 0.7, 2.0].iter().map(|&r| with(&|p| p.fold_readiness = r)).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TuneConfig {
    pub start: Params,
    pub field: BotSpec,
    pub training: Seeds,
    pub rounds: u32,
    /// Standard errors a change must win by to be adopted.
    pub min_z: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trial {
    pub round: u32,
    pub dimension: Dimension,
    pub candidate: Params,
    pub mean_difference: f64,
    pub stderr: f64,
    pub adopted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TuneResult {
    pub champion: Params,
    pub trials: Vec<Trial>,
    pub validation_seeds: Seeds,
    /// Champion against the starting parameters on held-out seeds.
    pub validation: ComparisonResult,
}

fn spec(
    name: &str,
    params: &Params,
) -> BotSpec {
    BotSpec::Heuristic {
        name: name.to_string(),
        params: params.clone(),
    }
}

pub fn tune(
    rules: &Ruleset,
    config: &TuneConfig,
) -> TuneResult {
    let mut champion = config.start.clone();
    let mut trials = Vec::new();
    for round in 0..config.rounds {
        let mut changed = false;
        for dimension in Dimension::ALL {
            let mut best: Option<(f64, Params)> = None;
            for candidate in dimension.variants(&champion).into_iter().filter(|c| *c != champion) {
                let c = compare(rules, &spec("candidate", &candidate), &spec("champion", &champion), &config.field, config.training);
                let wins = c.mean_difference > 0.0 && c.z() >= config.min_z;
                trials.push(Trial {
                    round,
                    dimension,
                    candidate: candidate.clone(),
                    mean_difference: c.mean_difference,
                    stderr: c.stderr,
                    adopted: false,
                });
                if wins && best.as_ref().is_none_or(|(d, _)| c.mean_difference > *d) {
                    best = Some((c.mean_difference, candidate));
                }
            }
            if let Some((_, winner)) = best {
                if let Some(trial) = trials.iter_mut().rev().find(|t| t.candidate == winner) {
                    trial.adopted = true;
                }
                champion = winner;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let validation_seeds = config.training.next_block();
    let validation = compare(rules, &spec("champion", &champion), &spec("start", &config.start), &config.field, validation_seeds).into();
    TuneResult {
        champion,
        trials,
        validation_seeds,
        validation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_change_only_their_dimension() {
        let base = Params::efficiency();
        for dimension in Dimension::ALL {
            let variants = dimension.variants(&base);
            assert!(variants.len() >= 3, "{dimension:?}");
            assert!(variants.contains(&base), "{dimension:?} includes the efficiency default");
        }
        let chows = Dimension::Chow.variants(&base);
        assert!(chows.iter().all(|p| Params { chow: base.chow, ..p.clone() } == base));
    }

    #[test]
    fn tuning_runs_and_validates_on_fresh_seeds() {
        let config = TuneConfig {
            start: Params {
                chow: ClaimRule::Always,
                pong: ClaimRule::Always,
                ..Params::efficiency()
            },
            field: BotSpec::preset("efficiency").unwrap(),
            training: Seeds { start: 0, count: 60 },
            rounds: 1,
            min_z: 2.0,
        };
        let result = tune(&Ruleset::family(), &config);
        assert_eq!(result.validation_seeds, Seeds { start: 60, count: 60 });
        assert!(!result.trials.is_empty());
        assert_eq!(result.trials.iter().filter(|t| t.adopted).count() > 0, result.champion != config.start);
        assert_eq!(result.validation.comparison.pairs, 240);
    }
}
