//! Only these types cross the client boundary. Persisted tables never do.

use mjers_core::advisor::heuristic::Assessment;
use mjers_core::advisor::montecarlo::Estimate;
use mjers_core::game::action::Action;
use mjers_core::game::snapshot::{
    Hand,
    Snapshot,
};
use mjers_core::hand::meld::Meld;
use mjers_core::rules::seat::Seat;
use mjers_core::rules::settlement::{
    HandResult,
    Settlement,
    WinSource,
};
use mjers_core::tile::{
    Bonus,
    Tile,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    StartHand,
    Act { action: Action, hand_number: u32, steps_seen: usize },
    NextHand { hand_number: u32 },
    Hint { samples: u32 },
    Ping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    State {
        table: String,
        version: u64,
        seats: [SeatView; 4],
        human_players: u8,
        password_protected: bool,
        scores: [i32; 4],
        hand_number: u32,
        steps: usize,
        observation: Option<Box<Snapshot>>,
    },
    Events {
        events: Vec<PublicEvent>,
    },
    HandOver {
        result: HandResult,
        settlement: SettlementView,
        revealed: Box<[Hand; 4]>,
    },
    Hint {
        advice: AdviceView,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
    Pong,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeatView {
    pub seat: Seat,
    pub name: Option<String>,
    pub kind: SeatKind,
    pub connected: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatKind {
    Open,
    Human,
    Bot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    IllegalAction,
    Stale,
    NotYourTurn,
    TableFull,
    BadToken,
    BadPassword,
    InvalidRoom,
    NotHost,
    NotReady,
    HandInProgress,
    Internal,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettlementView {
    pub win: [i32; 4],
    pub kongs: [i32; 4],
    pub total: [i32; 4],
}
impl From<Settlement> for SettlementView {
    fn from(value: Settlement) -> Self {
        Self {
            win: value.win,
            kongs: value.kongs,
            total: value.total(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AdviceView {
    pub recommended: Action,
    pub ranked: Vec<Assessment>,
    pub estimates: Vec<Estimate>,
}

/// Draws deliberately omit the tile; claim events describe resolved melds only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PublicEvent {
    Bonus { seat: Seat, bonus: Bonus },
    Draw { seat: Seat },
    Discard { seat: Seat, tile: Tile },
    Claim { seat: Seat, from: Seat, meld: Meld },
    Kong { seat: Seat, meld: Meld },
    Win { seat: Seat, source: WinSource, tile: Tile },
    Exhausted,
}
