//! Playing many independent hands in parallel.

use std::sync::Arc;

use mjers_core::bot::policy::{
    Policy,
    decide,
};
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game;
use mjers_core::game::action::GameError;
use mjers_core::game::rng::derive_seed;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use rayon::prelude::*;
use serde::{
    Deserialize,
    Serialize,
};

/// The strategy in each seat.
pub type Table = [BotSpec; 4];

/// A contiguous block of hand seeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seeds {
    pub start: u64,
    pub count: u64,
}

impl Seeds {
    pub fn iter(self) -> impl Iterator<Item = u64> {
        (0..self.count).map(move |i| self.start.wrapping_add(i))
    }

    /// The block immediately after this one.
    pub fn next_block(self) -> Seeds {
        Seeds {
            start: self.start.wrapping_add(self.count),
            count: self.count,
        }
    }
}

/// Plays one hand. The wall, the dealer, and every policy's randomness all
/// derive from `seed`, so the hand is identical wherever it runs.
pub fn play_hand(
    rules: &Arc<Ruleset>,
    table: &Table,
    seed: u64,
) -> Result<Game, HandFailure> {
    play_hand_watched(rules, table, seed, |_| {})
}

/// Like `play_hand`, calling `watch` after the deal and after every action.
pub fn play_hand_watched(
    rules: &Arc<Ruleset>,
    table: &Table,
    seed: u64,
    mut watch: impl FnMut(&Game),
) -> Result<Game, HandFailure> {
    let failed = |error| HandFailure { seed, error };
    let dealer = Seat::ALL[(seed % 4) as usize];
    let mut game = Game::new(rules.clone(), seed, dealer).map_err(failed)?;
    let mut policies: [Box<dyn Policy>; 4] = std::array::from_fn(|i| table[i].build(derive_seed(seed, i as u64)));
    watch(&game);
    while let Some(seat) = game.to_act().first() {
        let action = decide(&game, seat, &mut *policies[seat.index()]);
        game.apply(seat, action).map_err(failed)?;
        watch(&game);
    }
    Ok(game)
}

/// A hand that could not be played, with the seed that reproduces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandFailure {
    pub seed: u64,
    pub error: GameError,
}

impl std::fmt::Display for HandFailure {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(f, "hand with seed {} failed: {}", self.seed, self.error)
    }
}

impl std::error::Error for HandFailure {}

/// Runs `step` for every index in `0..count` in parallel, folding into `A`;
/// stops at the first failure.
pub(crate) fn par_try_fold<A, S, M>(
    count: u64,
    step: S,
    merge: M,
) -> Result<A, HandFailure>
where
    A: Default + Send,
    S: Fn(&mut A, u64) -> Result<(), HandFailure> + Sync + Send,
    M: Fn(A, A) -> A + Sync + Send,
{
    (0..count)
        .into_par_iter()
        .try_fold(A::default, |mut acc, i| {
            step(&mut acc, i)?;
            Ok(acc)
        })
        .try_reduce(A::default, |a, b| Ok(merge(a, b)))
}

/// Plays every seed in parallel, folding finished hands into `A`.
pub fn run<A, F, M>(
    rules: &Ruleset,
    table: &Table,
    seeds: Seeds,
    record: F,
    merge: M,
) -> Result<A, HandFailure>
where
    A: Default + Send,
    F: Fn(&mut A, &Game) + Sync + Send,
    M: Fn(A, A) -> A + Sync + Send,
{
    let rules = Arc::new(rules.clone());
    par_try_fold(
        seeds.count,
        |acc, i| {
            record(acc, &play_hand(&rules, table, seeds.start.wrapping_add(i))?);
            Ok(())
        },
        merge,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(name: &str) -> Table {
        std::array::from_fn(|_| BotSpec::preset(name).unwrap())
    }

    #[test]
    fn results_do_not_depend_on_threading() {
        let seeds = Seeds { start: 100, count: 64 };
        let totals = |_: ()| {
            run(
                &Ruleset::family(),
                &table("efficiency"),
                seeds,
                |acc: &mut Vec<(u64, [i32; 4])>, g| acc.push((g.seed().unwrap(), g.settlement().unwrap().total())),
                |mut a, b| {
                    a.extend(b);
                    a
                },
            )
            .unwrap()
        };
        let mut parallel = totals(());
        parallel.sort();
        let sequential: Vec<_> = seeds
            .iter()
            .map(|s| (s, play_hand(&Arc::new(Ruleset::family()), &table("efficiency"), s).unwrap().settlement().unwrap().total()))
            .collect();
        assert_eq!(parallel, sequential);
    }

    #[test]
    fn dealer_follows_the_seed() {
        let game = play_hand(&Arc::new(Ruleset::family()), &table("random"), 6).unwrap();
        assert_eq!(game.dealer(), Seat::ALL[2]);
    }

    #[test]
    fn next_block_is_disjoint() {
        let a = Seeds { start: 10, count: 5 };
        assert_eq!(a.next_block(), Seeds { start: 15, count: 5 });
        assert_eq!(Seeds { start: u64::MAX, count: 2 }.iter().collect::<Vec<_>>(), [u64::MAX, 0]);
    }
}
