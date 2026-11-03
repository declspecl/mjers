//! Local HTTP/WebSocket runtime backed by DynamoDB Local. Only active socket
//! handles live in memory; every room operation reads persisted state.

use std::collections::HashMap;
use std::sync::Arc;

use argon2::password_hash::SaltString;
use argon2::{
    Argon2,
    PasswordHasher,
};
use axum::extract::ws::{
    Message,
    WebSocket,
};
use axum::extract::{
    DefaultBodyLimit,
    Path,
    Query,
    State,
    WebSocketUpgrade,
};
use axum::http::StatusCode;
use axum::response::{
    IntoResponse,
    Response,
};
use axum::routing::{
    any,
    post,
};
use axum::{
    Json,
    Router,
};
use mjers_core::rules::seat::Seat;
use mjers_dynamodb::{
    StoreError,
    TableStore,
    local,
};
use mjers_table::protocol::{
    ClientMessage,
    ErrorCode,
    PublicEvent,
    ServerMessage,
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
use tokio::sync::{
    Mutex,
    mpsc,
    watch,
};

#[derive(Clone)]
struct AppState {
    store: TableStore,
    rooms: Arc<Mutex<HashMap<String, Arc<Mutex<Room>>>>>,
}
struct Room {
    sessions: [Option<Session>; 4],
    next_connection: u64,
}
struct Session {
    id: u64,
    outgoing: mpsc::Sender<String>,
    close: watch::Sender<bool>,
}
impl AppState {
    async fn room(
        &self,
        id: &str,
    ) -> Arc<Mutex<Room>> {
        self.rooms
            .lock()
            .await
            .entry(id.into())
            .or_insert_with(|| {
                Arc::new(Mutex::new(Room {
                    sessions: std::array::from_fn(|_| None),
                    next_connection: 0,
                }))
            })
            .clone()
    }
    async fn load(
        &self,
        id: &str,
    ) -> Result<Table, ApiError> {
        self.store.load(id).await?.ok_or_else(|| {
            ApiError(
                StatusCode::NOT_FOUND,
                TableError {
                    code: ErrorCode::InvalidRoom,
                    message: "Room not found. Check the invite code.".into(),
                },
            )
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRoom {
    name: String,
    #[serde(default = "default_humans")]
    human_players: u8,
    password: Option<String>,
    #[serde(default)]
    bot: BotPreset,
}
fn default_humans() -> u8 {
    2
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinRoom {
    name: String,
    password: Option<String>,
}
#[derive(Serialize)]
struct Joined {
    table: String,
    seat_token: String,
    seat: Seat,
    join_path: String,
}
#[derive(Deserialize)]
struct Connect {
    table: String,
    token: String,
}

struct ApiError(StatusCode, TableError);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(self.1.message())).into_response()
    }
}
impl From<TableError> for ApiError {
    fn from(error: TableError) -> Self {
        let status = match error.code {
            ErrorCode::BadPassword | ErrorCode::BadToken => StatusCode::FORBIDDEN,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        };
        Self(status, error)
    }
}
fn internal() -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        TableError {
            code: ErrorCode::Internal,
            message: "The room could not be processed. Try again.".into(),
        },
    )
}
fn invalid(message: &str) -> ApiError {
    ApiError(
        StatusCode::BAD_REQUEST,
        TableError {
            code: ErrorCode::InvalidRoom,
            message: message.into(),
        },
    )
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        eprintln!("Room storage operation failed: {error}");
        internal()
    }
}
fn busy() -> ApiError {
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        TableError {
            code: ErrorCode::Internal,
            message: "The room is busy. Try again.".into(),
        },
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Explicit local endpoint and dummy credentials: never load the user's AWS
    // profile or silently fall back to a cloud endpoint or in-memory storage.
    let endpoint = std::env::var("MJERS_DYNAMODB_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8000".into());
    let table_name = std::env::var("MJERS_DYNAMODB_TABLE").unwrap_or_else(|_| "mjers-local-tables".into());
    let client = local::client(&endpoint);
    local::ensure_table(&client, &table_name).await?;
    let state = AppState {
        store: TableStore::new(client, table_name),
        rooms: Arc::default(),
    };
    let app = Router::new()
        .route("/tables", post(create))
        .route("/tables/{id}/join", post(join))
        .route("/ws", any(connect))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(state);
    let address = std::env::var("MJERS_LISTEN").unwrap_or_else(|_| "127.0.0.1:3001".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    eprintln!("Local multiplayer server listening on {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn create(
    State(state): State<AppState>,
    Json(input): Json<CreateRoom>,
) -> Result<Json<Joined>, ApiError> {
    let password = input.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || {
        password
            .map(|password| {
                check_password(&password)?;
                let salt = SaltString::encode_b64(&random::<16>()?).map_err(|_| internal())?;
                Argon2::default().hash_password(password.as_bytes(), &salt).map(|hash| hash.to_string()).map_err(|_| internal())
            })
            .transpose()
    })
    .await
    .map_err(|_| internal())??;
    let token = token()?;
    for _ in 0..4 {
        let alphabet = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let id: String = random::<6>()?.iter().map(|byte| alphabet[(*byte as usize) % alphabet.len()] as char).collect();
        let table = Table::new(id.clone(), input.name.clone(), &token, input.human_players, password_hash.clone(), input.bot)?;
        match state.store.create(&table).await {
            Ok(()) => return Ok(Json(joined(id, token, Seat::ALL[0]))),
            Err(StoreError::Conflict) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(busy())
}

async fn join(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<JoinRoom>,
) -> Result<Json<Joined>, ApiError> {
    if let Some(password) = &input.password {
        check_password(password)?;
    }
    let token = token()?;
    let runtime = state.room(&id).await;
    let room = runtime.lock().await;
    for _ in 0..4 {
        let mut table = state.load(&id).await?;
        let version = table.version();
        let (name, password, seat_token) = (input.name.clone(), input.password.clone(), token.clone());
        // Argon2 verification stays off the async runtime.
        let (table, seat) = tokio::task::spawn_blocking(move || {
            let seat = table.join(name, &seat_token, password.as_deref())?;
            Ok::<_, TableError>((table, seat))
        })
        .await
        .map_err(|_| internal())??;
        match state.store.save(&table, version).await {
            Ok(()) => {
                room.broadcast(&table, &[])?;
                return Ok(Json(joined(id, token, seat)));
            },
            Err(StoreError::Conflict) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(busy())
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
fn check_password(password: &str) -> Result<(), ApiError> {
    if password.len() > 128 { Err(invalid("Use a password of at most 128 bytes.")) } else { Ok(()) }
}
fn random<const N: usize>() -> Result<[u8; N], ApiError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| internal())?;
    Ok(bytes)
}
fn token() -> Result<String, ApiError> {
    use std::fmt::Write;
    random::<32>()?.iter().try_fold(String::with_capacity(64), |mut text, byte| {
        write!(text, "{byte:02x}").map_err(|_| internal())?;
        Ok(text)
    })
}

async fn connect(
    State(state): State<AppState>,
    Query(input): Query<Connect>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let table = state.load(&input.table).await?;
    let seat = table.authenticate(&input.token)?;
    let room = state.room(&input.table).await;
    Ok(ws
        .max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |socket| serve_socket(state, input.table, room, seat, socket)))
}

async fn serve_socket(
    state: AppState,
    table_id: String,
    runtime: Arc<Mutex<Room>>,
    seat: Seat,
    mut socket: WebSocket,
) {
    let (outgoing, mut incoming) = mpsc::channel(64);
    let (close, mut closed) = watch::channel(false);
    let (id, initialized) = {
        let mut room = runtime.lock().await;
        room.next_connection += 1;
        let id = room.next_connection;
        if let Some(old) = room.sessions[seat.index()].replace(Session { id, outgoing, close }) {
            let _ = old.close.send(true);
        }
        let initialized = match state.load(&table_id).await {
            Ok(table) => room.broadcast(&table, &[]).is_ok(),
            Err(_) => false,
        };
        (id, initialized)
    };
    if initialized {
        loop {
            tokio::select! {
                biased;
                _ = closed.changed() => { let _ = socket.send(Message::Close(None)).await; break; },
                Some(text) = incoming.recv() => {
                    if socket.send(Message::Text(text.into())).await.is_err() { break; }
                },
                frame = socket.recv() => {
                    match frame {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(error) = process(&state, &table_id, &runtime, seat, id, &text).await {
                                let room = runtime.lock().await;
                                if room.active(seat, id) { let _ = room.send(seat, &error.1.message()); }
                            }
                        },
                        Some(Ok(Message::Ping(bytes))) => { if socket.send(Message::Pong(bytes)).await.is_err() { break; } },
                        Some(Ok(Message::Pong(_))) => {},
                        _ => break,
                    }
                }
            }
        }
    }
    let mut room = runtime.lock().await;
    if room.active(seat, id) {
        room.sessions[seat.index()] = None;
        if let Ok(table) = state.load(&table_id).await {
            let _ = room.broadcast(&table, &[]);
        }
    }
}

async fn process(
    state: &AppState,
    table_id: &str,
    runtime: &Arc<Mutex<Room>>,
    seat: Seat,
    id: u64,
    text: &str,
) -> Result<(), ApiError> {
    let message: ClientMessage = serde_json::from_str(text).map_err(|_| invalid("Invalid message. Reconnect and try again."))?;
    // This lock orders this process's broadcasts, but correctness of persisted
    // mutations relies on DynamoDB CAS, including writers in other processes.
    let room = runtime.lock().await;
    if !room.active(seat, id) {
        return Ok(());
    }
    let seed = u64::from_le_bytes(random::<8>()?);
    for _ in 0..4 {
        let mut table = state.load(table_id).await?;
        if matches!(message, ClientMessage::Ping | ClientMessage::Hint { .. }) {
            return room.send(seat, &table.read(seat, message)?);
        }
        let version = table.version();
        match table.handle(seat, message.clone(), seed) {
            Ok(change) => match state.store.save(&table, version).await {
                Ok(()) => return room.broadcast(&table, &change.events),
                Err(StoreError::Conflict) => continue,
                Err(error) => return Err(error.into()),
            },
            Err(error) => {
                room.send(seat, &error.message())?;
                if error.code == ErrorCode::Stale {
                    room.sync(&table, seat)?;
                }
                return Ok(());
            },
        }
    }
    Err(busy())
}
impl Room {
    fn active(
        &self,
        seat: Seat,
        id: u64,
    ) -> bool {
        self.sessions[seat.index()].as_ref().is_some_and(|session| session.id == id)
    }
    fn send(
        &self,
        seat: Seat,
        message: &ServerMessage,
    ) -> Result<(), ApiError> {
        if let Some(session) = &self.sessions[seat.index()] {
            let text = serde_json::to_string(message).map_err(|_| internal())?;
            if session.outgoing.try_send(text).is_err() {
                let _ = session.close.send(true);
            }
        }
        Ok(())
    }
    fn sync(
        &self,
        table: &Table,
        seat: Seat,
    ) -> Result<(), ApiError> {
        let connected = std::array::from_fn(|i| self.sessions[i].is_some());
        for message in table.messages(seat, connected)? {
            self.send(seat, &message)?;
        }
        Ok(())
    }
    fn broadcast(
        &self,
        table: &Table,
        events: &[PublicEvent],
    ) -> Result<(), ApiError> {
        for seat in Seat::ALL {
            if self.sessions[seat.index()].is_some() {
                if !events.is_empty() {
                    self.send(seat, &ServerMessage::Events { events: events.to_vec() })?;
                }
                self.sync(table, seat)?;
            }
        }
        Ok(())
    }
}
