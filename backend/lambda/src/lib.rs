//! API Gateway adapter. Game/room operations are shared with the local server;
//! sockets are authenticated and owned through DynamoDB connection records.

pub mod event;

use aws_sdk_apigatewaymanagement::Client;
use aws_sdk_apigatewaymanagement::primitives::Blob;
use mjers_dynamodb::StoreError;
use mjers_dynamodb::connections::{
    Connection,
    ConnectionStore,
};
use mjers_dynamodb::limits::RateLimiter;
use mjers_service::{
    CreateRoom,
    JoinRoom,
    RoomService,
    ServiceError,
};
use mjers_table::protocol::{
    ClientMessage,
    ErrorCode,
    PublicEvent,
    ServerMessage,
};
use mjers_table::{
    Table,
    TableError,
};

use crate::event::{
    GatewayEvent,
    GatewayResponse,
    invalid,
};

#[derive(Clone)]
pub struct Gateway {
    rooms: RoomService,
    connections: ConnectionStore,
    limits: RateLimiter,
    management: Client,
    site_origin: String,
    now: fn() -> Result<u64, StoreError>,
}
impl Gateway {
    pub fn new(
        rooms: RoomService,
        connections: ConnectionStore,
        limits: RateLimiter,
        management: Client,
        site_origin: String,
        now: fn() -> Result<u64, StoreError>,
    ) -> Self {
        Self {
            rooms,
            connections,
            limits,
            management,
            site_origin,
            now,
        }
    }
    pub async fn handle(
        &self,
        event: GatewayEvent,
    ) -> GatewayResponse {
        match self.dispatch(&event).await {
            Ok(response) => response,
            Err(error) => {
                let status = error.status();
                // Never log events, query strings, bodies, observations or credentials.
                if status >= 500 {
                    eprintln!("Backend operation failed (status {status})");
                }
                GatewayResponse::json(status, &error.client_error().message()).unwrap_or_else(|_| GatewayResponse {
                    status_code: 500,
                    headers: Default::default(),
                    body: String::new(),
                    is_base64_encoded: false,
                })
            },
        }
    }
    async fn dispatch(
        &self,
        event: &GatewayEvent,
    ) -> Result<GatewayResponse, ServiceError> {
        if let Some(http) = &event.request_context.http {
            let now = (self.now)()?;
            match (http.method.as_str(), event.raw_path.as_str()) {
                ("POST", "/tables") => {
                    self.limits.check(&format!("create:{}", http.source_ip), 5, now).await?;
                    let input: CreateRoom = serde_json::from_str(&event.body()?).map_err(|_| invalid("Invalid room settings."))?;
                    return GatewayResponse::json(200, &self.rooms.create(input).await?);
                },
                ("POST", path) if path.starts_with("/tables/") && path.ends_with("/join") => {
                    let id = &path[8..path.len() - 5];
                    if id.is_empty() || id.contains('/') {
                        return Err(invalid("Invalid room code."));
                    }
                    self.limits.check(&format!("join:{id}:{}", http.source_ip), 10, now).await?;
                    let input: JoinRoom = serde_json::from_str(&event.body()?).map_err(|_| invalid("Invalid join request."))?;
                    let (joined, table) = self.rooms.join(id, input).await?;
                    if self.broadcast(&table, &[]).await.is_err() {
                        eprintln!("Room joined; peers can recover with sync");
                    }
                    return GatewayResponse::json(200, &joined);
                },
                _ => {
                    return GatewayResponse::json(
                        404,
                        &ServerMessage::Error {
                            code: ErrorCode::InvalidRoom,
                            message: "Route not found.".into(),
                        },
                    );
                },
            }
        }
        let id = event.request_context.connection_id.as_deref().ok_or_else(|| invalid("Missing connection id."))?;
        match event.request_context.route_key.as_deref() {
            Some("$connect") => self.connect(event, id).await?,
            Some("$disconnect") => {
                if let Some(connection) = self.connections.disconnect(id).await?
                    && let Ok(table) = self.rooms.load(&connection.table).await
                {
                    self.broadcast(&table, &[]).await?;
                }
            },
            Some("$default") => {
                let Some(connection) = self.connections.load(id, (self.now)()?).await? else {
                    return Err(bad_token());
                };
                // Marking ready happens only after API Gateway has opened the socket.
                self.connections.mark_ready(&connection).await?;
                if let Err(error) = self.message(event, &connection).await {
                    self.send(&connection, &error.client_error().message()).await?;
                    if matches!(&error, ServiceError::Table(error) if error.code == ErrorCode::Stale) {
                        let table = self.rooms.load(&connection.table).await?;
                        self.sync(&table, &connection).await?;
                    }
                }
            },
            _ => return Err(invalid("Unknown WebSocket route.")),
        }
        GatewayResponse::json(200, &serde_json::json!({}))
    }
    async fn connect(
        &self,
        event: &GatewayEvent,
        id: &str,
    ) -> Result<(), ServiceError> {
        if event.header("origin").is_some_and(|origin| origin != self.site_origin) {
            return Err(bad_token());
        }
        let now = (self.now)()?;
        self.limits
            .check(
                &format!("connect:{}", event.request_context.identity.as_ref().map(|identity| identity.source_ip.as_str()).unwrap_or_default()),
                20,
                now,
            )
            .await?;
        let table = self.rooms.load(event.query("table")?).await?;
        let seat = table.authenticate(event.query("token")?)?;
        let connection = Connection {
            id: id.into(),
            table: table.id().into(),
            seat,
            expires_at: now + 7260,
            ready: false,
        };
        for _ in 0..4 {
            match self.connections.connect(&connection).await {
                Ok(old) => {
                    if let Some(old) = old {
                        // Ownership is already revoked even if closing the old socket fails.
                        let _ = self.management.delete_connection().connection_id(old).send().await;
                    }
                    return Ok(());
                },
                Err(StoreError::Conflict) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ServiceError::Busy)
    }
    async fn message(
        &self,
        event: &GatewayEvent,
        connection: &Connection,
    ) -> Result<(), ServiceError> {
        self.limits.check(&format!("message:{}", connection.id), 120, (self.now)()?).await?;
        let message: ClientMessage = serde_json::from_str(&event.body()?).map_err(|_| invalid("Invalid message. Reconnect and try again."))?;
        match message {
            ClientMessage::Sync => {
                let table = self.rooms.load(&connection.table).await?;
                self.broadcast(&table, &[]).await
            },
            ClientMessage::Ping | ClientMessage::Hint { .. } => {
                let table = self.rooms.load(&connection.table).await?;
                self.send(connection, &table.read(connection.seat, message)?).await
            },
            _ => {
                let (table, events) = self.rooms.command_connected(&connection.table, connection.seat, message, &self.connections.guard(connection)).await?;
                if self.broadcast(&table, &events).await.is_err() {
                    eprintln!("Action committed; peers can recover with sync");
                }
                Ok(())
            },
        }
    }
    async fn sync(
        &self,
        table: &Table,
        connection: &Connection,
    ) -> Result<(), ServiceError> {
        let active = self.connections.list(table.id(), (self.now)()?).await?;
        let connected = std::array::from_fn(|i| active.iter().any(|connection| connection.seat.index() == i));
        for message in table.messages(connection.seat, connected)? {
            self.send(connection, &message).await?;
        }
        Ok(())
    }
    async fn broadcast(
        &self,
        table: &Table,
        events: &[PublicEvent],
    ) -> Result<(), ServiceError> {
        let active = self.connections.list(table.id(), (self.now)()?).await?;
        let connected = std::array::from_fn(|i| active.iter().any(|connection| connection.seat.index() == i));
        for connection in &active {
            if !events.is_empty() {
                self.send(
                    connection,
                    &ServerMessage::Events {
                        version: table.version(),
                        hand_number: table.hand_number(),
                        events: events.to_vec(),
                    },
                )
                .await?;
            }
            for message in table.messages(connection.seat, connected)? {
                self.send(connection, &message).await?;
            }
        }
        Ok(())
    }
    async fn send(
        &self,
        connection: &Connection,
        message: &ServerMessage,
    ) -> Result<(), ServiceError> {
        // Recheck generation before sending private data to a replaced connection.
        if self.connections.load(&connection.id, (self.now)()?).await?.is_none() {
            return Ok(());
        }
        let data = serde_json::to_vec(message).map_err(|_| ServiceError::Runtime)?;
        if data.len() > 32 * 1024 {
            return Err(ServiceError::Runtime);
        }
        match self.management.post_to_connection().connection_id(&connection.id).data(Blob::new(data)).send().await {
            Ok(_) => Ok(()),
            Err(error) if error.as_service_error().is_some_and(|error| error.is_gone_exception()) => {
                self.connections.disconnect(&connection.id).await?;
                Ok(())
            },
            // A committed action must not be reported as uncommitted just because
            // a callback failed. The client can sync to recover a full snapshot.
            Err(_) => {
                eprintln!("Socket callback failed; client can recover with sync");
                Ok(())
            },
        }
    }
}
fn bad_token() -> ServiceError {
    TableError {
        code: ErrorCode::BadToken,
        message: "Your seat could not be restored. Join the room again.".into(),
    }
    .into()
}
