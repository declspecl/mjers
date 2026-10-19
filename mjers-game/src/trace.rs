//! Event logs and replayable traces.

use std::fmt;
use std::sync::Arc;

use mjers_hand::meld::Meld;
use mjers_rules::ruleset::Ruleset;
use mjers_rules::seat::Seat;
use mjers_rules::settlement::{
    HandResult,
    WinSource,
};
use mjers_tile::counts::TileCounts;
use mjers_tile::tile::{
    Bonus,
    Tile,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::action::{
    Action,
    GameError,
};
use crate::game::Game;
use crate::wall::{
    WALL_SIZE,
    Wall,
    WallTile,
};

pub const TRACE_VERSION: u32 = 1;

/// Everything that happened, including hidden information. For replays and
/// analysis only; policies see observations instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Deal {
        seat: Seat,
        tiles: TileCounts,
    },
    Bonus {
        seat: Seat,
        bonus: Bonus,
    },
    Draw {
        seat: Seat,
        tile: Tile,
    },
    Discard {
        seat: Seat,
        tile: Tile,
    },
    Claim {
        seat: Seat,
        from: Seat,
        meld: Meld,
    },
    Kong {
        seat: Seat,
        meld: Meld,
    },
    /// `tile` completed the hand, drawn or claimed.
    Win {
        seat: Seat,
        source: WinSource,
        tile: Tile,
    },
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub seat: Seat,
    pub action: Action,
}

/// A complete record of one hand that replays exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub version: u32,
    pub rules: Ruleset,
    pub dealer: Seat,
    /// The seed the wall was shuffled from, if any.
    pub seed: Option<u64>,
    pub wall: Vec<WallTile>,
    pub steps: Vec<Step>,
    pub events: Vec<Event>,
    pub result: Option<HandResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    Version(u32),
    Wall(usize),
    Step { index: usize, error: GameError },
    Diverged,
}

impl fmt::Display for ReplayError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            ReplayError::Version(v) => write!(f, "unsupported trace version {v}"),
            ReplayError::Wall(n) => write!(f, "wall has {n} tiles, expected {WALL_SIZE}"),
            ReplayError::Step { index, error } => write!(f, "step {index}: {error}"),
            ReplayError::Diverged => write!(f, "replay does not reproduce the recorded events"),
        }
    }
}

impl std::error::Error for ReplayError {}

impl Game {
    pub fn trace(&self) -> Trace {
        Trace {
            version: TRACE_VERSION,
            rules: self.rules().clone(),
            dealer: self.dealer(),
            seed: self.seed(),
            wall: self.initial_wall().undrawn().to_vec(),
            steps: self.steps().to_vec(),
            events: self.events().to_vec(),
            result: self.result().cloned(),
        }
    }
}

impl Trace {
    /// Re-plays every step from the recorded wall and checks the outcome.
    pub fn replay(&self) -> Result<Game, ReplayError> {
        if self.version != TRACE_VERSION {
            return Err(ReplayError::Version(self.version));
        }
        let wall: [WallTile; WALL_SIZE] = self.wall.clone().try_into().map_err(|w: Vec<_>| ReplayError::Wall(w.len()))?;
        let reserved = self.rules.reserved_tiles();
        let mut game = Game::from_wall(Arc::new(self.rules.clone()), Wall::from_order(wall, reserved), self.dealer);
        for (index, step) in self.steps.iter().enumerate() {
            game.apply(step.seat, step.action).map_err(|error| ReplayError::Step { index, error })?;
        }
        if game.events() != self.events || game.result() != self.result.as_ref() {
            return Err(ReplayError::Diverged);
        }
        Ok(game)
    }
}
