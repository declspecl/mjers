//! Shared orchestration for both runtimes. Persistence and credentials are
//! explicit; the pure table remains the owner of game and room rules.

use argon2::password_hash::SaltString;
use argon2::{
    Argon2,
    PasswordHasher,
};
use mjers_core::rules::seat::Seat;
use mjers_dynamodb::connections::ConnectionGuard;
use mjers_dynamodb::{
    StoreError,
    TableStore,
};
use mjers_table::protocol::{
    ClientMessage,
    ErrorCode,
    PublicEvent,
};
use mjers_table::{
    BotPreset,
    Table,
    TableError,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRoom {
    pub name: String,
    #[serde(default = "default_humans")]
    pub human_players: u8,
    pub password: Option<String>,
    #[serde(default)]
    pub bot: BotPreset,
}
fn default_humans() -> u8 {
    2
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinRoom {
    pub name: String,
    pub password: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct Joined {
    pub table: String,
    pub seat_token: String,
    pub seat: Seat,
    pub join_path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Table(#[from] TableError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("room not found")]
    NotFound,
    #[error("room is busy")]
    Busy,
    #[error("runtime operation failed")]
    Runtime,
}
impl ServiceError {
    pub fn status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Busy => 503,
            Self::Store(StoreError::RateLimited) => 429,
            Self::Store(StoreError::ConnectionLost) => 403,
            Self::Table(error) => match error.code {
                ErrorCode::BadPassword | ErrorCode::BadToken => 403,
                ErrorCode::Internal => 500,
                _ => 400,
            },
            _ => 500,
        }
    }
    pub fn client_error(&self) -> TableError {
        match self {
            Self::Table(error) => error.clone(),
            Self::Store(StoreError::ConnectionLost) => TableError {
                code: ErrorCode::BadToken,
                message: "This connection was replaced. Reconnect to restore your seat.".into(),
            },
            Self::Store(StoreError::RateLimited) => TableError {
                code: ErrorCode::RateLimited,
                message: "Too many requests. Try again in a minute.".into(),
            },
            Self::NotFound => TableError {
                code: ErrorCode::InvalidRoom,
                message: "Room not found. Check the invite code.".into(),
            },
            Self::Busy => TableError {
                code: ErrorCode::Internal,
                message: "The room is busy. Try again.".into(),
            },
            _ => TableError {
                code: ErrorCode::Internal,
                message: "The room could not be processed. Try again.".into(),
            },
        }
    }
}

#[derive(Clone)]
pub struct RoomService {
    tables: TableStore,
}
impl RoomService {
    pub fn new(tables: TableStore) -> Self {
        Self { tables }
    }
    pub async fn load(
        &self,
        id: &str,
    ) -> Result<Table, ServiceError> {
        self.tables.load(id).await?.ok_or(ServiceError::NotFound)
    }
    pub async fn create(
        &self,
        input: CreateRoom,
    ) -> Result<Joined, ServiceError> {
        let password = input.password;
        let password_hash = tokio::task::spawn_blocking(move || {
            password
                .map(|password| {
                    check_password(&password)?;
                    let salt = SaltString::encode_b64(&random::<16>()?).map_err(|_| ServiceError::Runtime)?;
                    Argon2::default()
                        .hash_password(password.as_bytes(), &salt)
                        .map(|hash| hash.to_string())
                        .map_err(|_| ServiceError::Runtime)
                })
                .transpose()
        })
        .await
        .map_err(|_| ServiceError::Runtime)??;
        let token = token()?;
        for _ in 0..4 {
            let alphabet = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
            let id: String = random::<6>()?.iter().map(|byte| alphabet[(*byte as usize) % alphabet.len()] as char).collect();
            let table = Table::new(id.clone(), input.name.clone(), &token, input.human_players, password_hash.clone(), input.bot)?;
            match self.tables.create(&table).await {
                Ok(()) => return Ok(joined(id, token, Seat::ALL[0])),
                Err(StoreError::Conflict) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ServiceError::Busy)
    }
    pub async fn join(
        &self,
        id: &str,
        input: JoinRoom,
    ) -> Result<(Joined, Table), ServiceError> {
        if let Some(password) = &input.password {
            check_password(password)?;
        }
        let token = token()?;
        for _ in 0..4 {
            let mut table = self.load(id).await?;
            let version = table.version();
            let (name, password, seat_token) = (input.name.clone(), input.password.clone(), token.clone());
            let (table, seat) = tokio::task::spawn_blocking(move || {
                let seat = table.join(name, &seat_token, password.as_deref())?;
                Ok::<_, TableError>((table, seat))
            })
            .await
            .map_err(|_| ServiceError::Runtime)??;
            match self.tables.save(&table, version).await {
                Ok(()) => return Ok((joined(id.into(), token, seat), table)),
                Err(StoreError::Conflict) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ServiceError::Busy)
    }
    pub async fn command(
        &self,
        id: &str,
        seat: Seat,
        message: ClientMessage,
    ) -> Result<(Table, Vec<PublicEvent>), ServiceError> {
        self.apply(id, seat, message, None).await
    }
    pub async fn command_connected(
        &self,
        id: &str,
        seat: Seat,
        message: ClientMessage,
        guard: &ConnectionGuard,
    ) -> Result<(Table, Vec<PublicEvent>), ServiceError> {
        self.apply(id, seat, message, Some(guard)).await
    }
    async fn apply(
        &self,
        id: &str,
        seat: Seat,
        message: ClientMessage,
        guard: Option<&ConnectionGuard>,
    ) -> Result<(Table, Vec<PublicEvent>), ServiceError> {
        let seed = u64::from_le_bytes(random::<8>()?);
        for _ in 0..4 {
            let mut table = self.load(id).await?;
            let version = table.version();
            let change = table.handle(seat, message.clone(), seed)?;
            let saved = match guard {
                Some(guard) => self.tables.save_connected(&table, version, guard).await,
                None => self.tables.save(&table, version).await,
            };
            match saved {
                Ok(()) => return Ok((table, change.events)),
                Err(StoreError::Conflict) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ServiceError::Busy)
    }
}
fn joined(
    id: String,
    token: String,
    seat: Seat,
) -> Joined {
    Joined {
        join_path: format!("/?room={id}"),
        table: id,
        seat_token: token,
        seat,
    }
}
fn check_password(password: &str) -> Result<(), ServiceError> {
    if password.len() > 128 {
        return Err(TableError {
            code: ErrorCode::InvalidRoom,
            message: "Use a password of at most 128 bytes.".into(),
        }
        .into());
    }
    Ok(())
}
fn random<const N: usize>() -> Result<[u8; N], ServiceError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| ServiceError::Runtime)?;
    Ok(bytes)
}
fn token() -> Result<String, ServiceError> {
    use std::fmt::Write;
    random::<32>()?.iter().try_fold(String::with_capacity(64), |mut text, byte| {
        write!(text, "{byte:02x}").map_err(|_| ServiceError::Runtime)?;
        Ok(text)
    })
}
