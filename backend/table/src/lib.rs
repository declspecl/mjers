//! Pure room operations. Random bytes, persistence, presence and transport belong
//! to adapters. A failed command leaves the persisted table unchanged.

pub mod protocol;
#[cfg(test)]
mod tests;

use std::fmt;
use std::sync::Arc;

use argon2::{
    Argon2,
    PasswordHash,
    PasswordVerifier,
};
use mjers_core::advisor::{
    AdviceConfig,
    advise,
};
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game;
use mjers_core::game::action::GameError;
use mjers_core::game::rng::derive_seed;
use mjers_core::game::snapshot::{
    Hand,
    Snapshot,
};
use mjers_core::game::trace::{
    Event,
    Step,
};
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};
use subtle::ConstantTimeEq;

use crate::protocol::{
    AdviceView,
    ClientMessage,
    ErrorCode,
    PublicEvent,
    SeatKind,
    SeatView,
    ServerMessage,
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotPreset {
    Random,
    #[default]
    Efficiency,
    Defensive,
}
impl BotPreset {
    fn spec(self) -> BotSpec {
        match self {
            Self::Random => BotSpec::Random { name: "random".into() },
            Self::Efficiency => BotSpec::Heuristic {
                name: "efficiency".into(),
                params: mjers_core::bot::params::Params::efficiency(),
            },
            Self::Defensive => BotSpec::Heuristic {
                name: "defensive".into(),
                params: mjers_core::bot::params::Params::defensive(),
            },
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum SeatSlot {
    Open,
    Human {
        name: String,
        token_hash: [u8; 32],
        #[serde(default)]
        autoplay: bool,
    },
    Bot {
        preset: BotPreset,
    },
}

/// Server-only persistence model: contains credentials and hidden hand data.
#[derive(Clone, Serialize, Deserialize)]
pub struct Table {
    id: String,
    version: u64,
    human_players: u8,
    password_hash: Option<String>,
    seats: [SeatSlot; 4],
    hand: Option<HandRecord>,
    history: Vec<HandRecord>,
    #[serde(default)]
    archived_hands: u32,
    scores: [i32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandRecord {
    seed: u64,
    dealer: Seat,
    steps: Vec<Step>,
}
impl HandRecord {
    pub fn replay(&self) -> Result<Game, TableError> {
        let mut game = Game::new(Arc::new(Ruleset::family()), self.seed, self.dealer)?;
        for step in &self.steps {
            game.apply(step.seat, step.action)?;
        }
        Ok(game)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableError {
    pub code: ErrorCode,
    pub message: String,
}
impl TableError {
    fn new(
        code: ErrorCode,
        message: &str,
    ) -> Self {
        Self { code, message: message.into() }
    }
    pub fn message(&self) -> ServerMessage {
        ServerMessage::Error {
            code: self.code,
            message: self.message.clone(),
        }
    }
}
impl fmt::Display for TableError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for TableError {}
impl From<GameError> for TableError {
    fn from(error: GameError) -> Self {
        let code = match error {
            GameError::NotAsked(_) => ErrorCode::NotYourTurn,
            GameError::Illegal { .. } => ErrorCode::IllegalAction,
            GameError::HandOver => ErrorCode::NotReady,
            _ => ErrorCode::Internal,
        };
        Self {
            code,
            message: if code == ErrorCode::Internal {
                "The hand could not be processed.".into()
            } else {
                error.to_string()
            },
        }
    }
}

pub struct Change {
    pub events: Vec<PublicEvent>,
}

impl Table {
    /// The adapter generates the room code, token, and optional Argon2 password
    /// hash with OS randomness. No raw credentials are retained.
    pub fn new(
        id: String,
        name: String,
        token: &str,
        human_players: u8,
        password_hash: Option<String>,
        bot: BotPreset,
    ) -> Result<Self, TableError> {
        if !(1..=4).contains(&human_players) {
            return Err(TableError::new(ErrorCode::InvalidRoom, "Choose between one and four human players."));
        }
        if let Some(hash) = &password_hash {
            PasswordHash::new(hash).map_err(|_| TableError::new(ErrorCode::Internal, "Invalid password verifier."))?;
        }
        let name = username(name)?;
        let seats = std::array::from_fn(|index| {
            if index == 0 {
                SeatSlot::Human {
                    name: name.clone(),
                    token_hash: token_hash(token),
                    autoplay: false,
                }
            } else if index < human_players as usize {
                SeatSlot::Open
            } else {
                SeatSlot::Bot { preset: bot }
            }
        });
        Ok(Self {
            id,
            version: 0,
            human_players,
            password_hash,
            seats,
            hand: None,
            history: Vec::new(),
            archived_hands: 0,
            scores: [0; 4],
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn hand_number(&self) -> u32 {
        self.archived_hands + self.history.len() as u32 + u32::from(self.hand.is_some())
    }
    pub fn steps(&self) -> usize {
        self.hand.as_ref().map_or(0, |hand| hand.steps.len())
    }

    /// Completed hands may be archived by persistence, never sent before hand_over.
    pub fn completed_hand(&self) -> Result<Option<&HandRecord>, TableError> {
        match &self.hand {
            Some(hand) if hand.replay()?.is_over() => Ok(Some(hand)),
            _ => Ok(None),
        }
    }

    pub fn authenticate(
        &self,
        token: &str,
    ) -> Result<Seat, TableError> {
        let hash = token_hash(token);
        Seat::ALL
            .into_iter()
            .find(|seat| match &self.seats[seat.index()] {
                SeatSlot::Human { token_hash, .. } => bool::from(hash.ct_eq(token_hash)),
                _ => false,
            })
            .ok_or_else(|| TableError::new(ErrorCode::BadToken, "Your seat could not be restored. Join the room again."))
    }

    pub fn join(
        &mut self,
        name: String,
        token: &str,
        password: Option<&str>,
    ) -> Result<Seat, TableError> {
        if let Some(hash) = &self.password_hash {
            let parsed = PasswordHash::new(hash).map_err(|_| TableError::new(ErrorCode::Internal, "Invalid password verifier."))?;
            Argon2::default()
                .verify_password(password.unwrap_or_default().as_bytes(), &parsed)
                .map_err(|_| TableError::new(ErrorCode::BadPassword, "The room password is incorrect."))?;
        }
        if self.hand.is_some() {
            return Err(TableError::new(ErrorCode::HandInProgress, "This room has already started."));
        }
        let name = username(name)?;
        let seat = Seat::ALL
            .into_iter()
            .find(|seat| matches!(self.seats[seat.index()], SeatSlot::Open))
            .ok_or_else(|| TableError::new(ErrorCode::TableFull, "All human seats are taken."))?;
        self.seats[seat.index()] = SeatSlot::Human {
            name,
            token_hash: token_hash(token),
            autoplay: false,
        };
        self.version += 1;
        Ok(seat)
    }

    /// `seed` is consumed only when starting a hand. Commands execute on a
    /// candidate so engine failures cannot leave partial actions or scores.
    pub fn handle(
        &mut self,
        seat: Seat,
        message: ClientMessage,
        seed: u64,
    ) -> Result<Change, TableError> {
        self.require_human(seat)?;
        let mut candidate = self.clone();
        let events = candidate.apply(seat, message, seed)?;
        candidate.version += 1;
        *self = candidate;
        Ok(Change { events })
    }

    fn apply(
        &mut self,
        seat: Seat,
        message: ClientMessage,
        seed: u64,
    ) -> Result<Vec<PublicEvent>, TableError> {
        let mut game = match message {
            ClientMessage::StartHand | ClientMessage::NextHand { .. } => {
                if seat != Seat::ALL[0] {
                    return Err(TableError::new(ErrorCode::NotHost, "Only the host can start a hand."));
                }
                let dealer = match (&self.hand, &message) {
                    (None, ClientMessage::StartHand) => Seat::ALL[0],
                    (Some(hand), ClientMessage::NextHand { hand_number }) => {
                        if *hand_number != self.hand_number() {
                            return Err(TableError::new(ErrorCode::Stale, "The hand has changed. Use the refreshed state."));
                        }
                        if !hand.replay()?.is_over() {
                            return Err(TableError::new(ErrorCode::HandInProgress, "Finish this hand first."));
                        }
                        let dealer = hand.dealer.next();
                        self.history.push(hand.clone());
                        while self.history.len() > 4 {
                            self.history.remove(0);
                            self.archived_hands += 1;
                        }
                        dealer
                    },
                    _ => return Err(TableError::new(ErrorCode::NotReady, "The room is not ready for that action.")),
                };
                if self.seats.iter().any(|slot| matches!(slot, SeatSlot::Open)) {
                    return Err(TableError::new(ErrorCode::NotReady, "Wait for the remaining human players to join."));
                }
                Game::new(Arc::new(Ruleset::family()), seed, dealer)?
            },
            ClientMessage::Autoplay { enabled, hand_number, steps_seen } => {
                if hand_number != self.hand_number() || steps_seen != self.steps() {
                    return Err(TableError::new(ErrorCode::Stale, "The hand has changed. Use the refreshed state."));
                }
                if let SeatSlot::Human { autoplay, .. } = &mut self.seats[seat.index()] {
                    *autoplay = enabled;
                }
                let Some(hand) = &self.hand else {
                    return Ok(Vec::new());
                };
                let mut game = hand.replay()?;
                if game.is_over() {
                    return Ok(Vec::new());
                }
                let event_start = game.events().len();
                self.advance_bots(&mut game)?;
                let events = public_events(&game.events()[event_start..]);
                self.save_game(&game);
                return Ok(events);
            },
            ClientMessage::Act { action, hand_number, steps_seen } => {
                if hand_number != self.hand_number() || steps_seen != self.steps() {
                    return Err(TableError::new(ErrorCode::Stale, "The hand has changed. Use the refreshed state."));
                }
                let mut game = self.game()?;
                let event_start = game.events().len();
                game.apply(seat, action)?;
                self.advance_bots(&mut game)?;
                let events = public_events(&game.events()[event_start..]);
                self.save_game(&game);
                return Ok(events);
            },
            _ => return Err(TableError::new(ErrorCode::InvalidRoom, "Use the read-only handler for hints and keepalives.")),
        };
        self.advance_bots(&mut game)?;
        let events = public_events(game.events());
        self.save_game(&game);
        Ok(events)
    }

    fn advance_bots(
        &self,
        game: &mut Game,
    ) -> Result<(), TableError> {
        // Act on bot claims even when a human also has a decision. Waiting for
        // an all-bot to_act set would leave mixed claim windows stuck.
        loop {
            let bot = game.to_act().iter().find_map(|seat| match self.seats[seat.index()] {
                SeatSlot::Bot { preset } => Some((seat, preset)),
                SeatSlot::Human { autoplay: true, .. } => Some((seat, BotPreset::Efficiency)),
                _ => None,
            });
            let Some((seat, preset)) = bot else {
                break;
            };
            // Each fresh decision has its own deterministic stream. The log is
            // authoritative; replay never re-asks policies for historical steps.
            let seed = derive_seed(derive_seed(game.seed().unwrap_or_default(), seat.index() as u64), game.steps().len() as u64);
            let obs = game.observe(seat);
            let action = preset.spec().build(seed).act(&obs, &obs.legal_actions());
            game.apply(seat, action)?;
        }
        Ok(())
    }

    fn save_game(
        &mut self,
        game: &Game,
    ) {
        if let Some(settlement) = game.settlement() {
            for (score, delta) in self.scores.iter_mut().zip(settlement.total()) {
                *score += delta;
            }
        }
        self.hand = Some(HandRecord {
            seed: game.seed().unwrap_or_default(),
            dealer: game.dealer(),
            steps: game.steps().to_vec(),
        });
    }

    fn require_human(
        &self,
        seat: Seat,
    ) -> Result<(), TableError> {
        if matches!(self.seats.get(seat.index()), Some(SeatSlot::Human { .. })) {
            Ok(())
        } else {
            Err(TableError::new(ErrorCode::BadToken, "This seat is not occupied by a human."))
        }
    }
    fn game(&self) -> Result<Game, TableError> {
        self.hand
            .as_ref()
            .ok_or_else(|| TableError::new(ErrorCode::NotReady, "The host has not started the game yet."))?
            .replay()
    }

    pub fn read(
        &self,
        seat: Seat,
        message: ClientMessage,
    ) -> Result<ServerMessage, TableError> {
        self.require_human(seat)?;
        match message {
            ClientMessage::Ping => Ok(ServerMessage::Pong),
            ClientMessage::Hint { samples: 0 } => {
                let game = self.game()?;
                if !game.to_act().contains(seat) {
                    return Err(TableError::new(ErrorCode::NotYourTurn, "Wait for your turn before asking for a hint."));
                }
                let advice = advise(
                    &game.observe(seat),
                    &AdviceConfig {
                        samples: 0,
                        ..AdviceConfig::default()
                    },
                );
                Ok(ServerMessage::Hint {
                    version: self.version,
                    hand_number: self.hand_number(),
                    advice: AdviceView {
                        recommended: advice.best(),
                        ranked: advice.ranked,
                        estimates: advice.estimates,
                    },
                })
            },
            _ => Err(TableError::new(ErrorCode::InvalidRoom, "Only instant hints are supported.")),
        }
    }

    pub fn messages(
        &self,
        seat: Seat,
        connected: [bool; 4],
    ) -> Result<Vec<ServerMessage>, TableError> {
        self.require_human(seat)?;
        let game = self.hand.as_ref().map(HandRecord::replay).transpose()?;
        let seats = Seat::ALL.map(|seat| {
            let (kind, name) = match &self.seats[seat.index()] {
                SeatSlot::Open => (SeatKind::Open, None),
                SeatSlot::Human { name, .. } => (SeatKind::Human, Some(name.clone())),
                SeatSlot::Bot { preset } => (SeatKind::Bot, Some(preset.spec().name().into())),
            };
            SeatView {
                seat,
                name,
                kind,
                connected: connected[seat.index()],
                autoplay: matches!(self.seats[seat.index()], SeatSlot::Human { autoplay: true, .. }),
            }
        });
        let mut messages = vec![ServerMessage::State {
            table: self.id.clone(),
            version: self.version,
            seats,
            human_players: self.human_players,
            password_protected: self.password_hash.is_some(),
            scores: self.scores,
            hand_number: self.hand_number(),
            steps: self.steps(),
            observation: game.as_ref().map(|game| Box::new(Snapshot::new(game, seat))),
        }];
        if let Some(game) = game
            && let (Some(result), Some(settlement)) = (game.result(), game.settlement())
        {
            messages.push(ServerMessage::HandOver {
                version: self.version,
                hand_number: self.hand_number(),
                result: result.clone(),
                settlement: settlement.into(),
                revealed: Box::new(Seat::ALL.map(|seat| Hand::new(game.hand(seat)))),
            });
        }
        Ok(messages)
    }
}

fn token_hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}
fn username(name: String) -> Result<String, TableError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(TableError::new(ErrorCode::InvalidRoom, "Choose a username between 1 and 40 characters."));
    }
    Ok(name.into())
}
fn public_events(events: &[Event]) -> Vec<PublicEvent> {
    events
        .iter()
        .filter_map(|event| {
            Some(match *event {
                Event::Deal { .. } => return None,
                Event::Bonus { seat, bonus } => PublicEvent::Bonus { seat, bonus },
                Event::Draw { seat, .. } => PublicEvent::Draw { seat },
                Event::Discard { seat, tile } => PublicEvent::Discard { seat, tile },
                Event::Claim { seat, from, meld } => PublicEvent::Claim { seat, from, meld },
                Event::Kong { seat, meld } => PublicEvent::Kong { seat, meld },
                Event::Win { seat, source, tile } => PublicEvent::Win { seat, source, tile },
                Event::Exhausted => PublicEvent::Exhausted,
            })
        })
        .collect()
}
