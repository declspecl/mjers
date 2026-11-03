//! Real Lambda event dispatch and signed AWS SDK requests against DynamoDB Local
//! and a local API Gateway management endpoint. No mocked storage implementation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

use aws_sdk_apigatewaymanagement::Client;
use aws_sdk_apigatewaymanagement::config::{
    BehaviorVersion,
    Credentials,
    Region,
};
use axum::extract::{
    Path,
    State,
};
use axum::http::StatusCode;
use axum::response::{
    IntoResponse,
    Response,
};
use axum::routing::post;
use axum::{
    Json,
    Router,
};
use mjers_core::rules::seat::Seat;
use mjers_dynamodb::connections::{
    Connection,
    ConnectionStore,
};
use mjers_dynamodb::limits::RateLimiter;
use mjers_dynamodb::{
    StoreError,
    TableStore,
    epoch_seconds,
    local,
};
use mjers_lambda::Gateway;
use mjers_lambda::event::{
    GatewayEvent,
    GatewayResponse,
};
use mjers_service::{
    Joined,
    RoomService,
};
use mjers_table::protocol::ClientMessage;
use serde_json::{
    Value,
    json,
};

#[derive(Clone, Default)]
struct Callbacks {
    messages: Arc<Mutex<Vec<(String, Value)>>>,
    closed: Arc<Mutex<Vec<String>>>,
    gone: Arc<Mutex<HashSet<String>>>,
}
async fn callback(
    State(state): State<Callbacks>,
    Path(id): Path<String>,
    body: String,
) -> Response {
    if state.gone.lock().unwrap().contains(&id) {
        return (StatusCode::GONE, [("x-amzn-errortype", "GoneException")], Json(json!({"message":"gone"}))).into_response();
    }
    state.messages.lock().unwrap().push((id, serde_json::from_str(&body).unwrap()));
    StatusCode::OK.into_response()
}
async fn close(
    State(state): State<Callbacks>,
    Path(id): Path<String>,
) -> StatusCode {
    state.closed.lock().unwrap().push(id);
    StatusCode::NO_CONTENT
}
fn http(
    path: &str,
    ip: &str,
    body: Value,
) -> GatewayEvent {
    serde_json::from_value(json!({"version":"2.0","rawPath":path,"requestContext":{"http":{"method":"POST","sourceIp":ip}},"body":body.to_string()})).unwrap()
}
fn socket(
    route: &str,
    id: &str,
    query: Value,
    body: Value,
) -> GatewayEvent {
    serde_json::from_value(json!({"requestContext":{"routeKey":route,"connectionId":id,"identity":{"sourceIp":"127.0.0.1"}},"queryStringParameters":query,"headers":{"Origin":"https://game.example"},"body":body.to_string()})).unwrap()
}
async fn message(
    gateway: &Gateway,
    id: &str,
    body: Value,
) -> GatewayResponse {
    gateway.handle(socket("$default", id, json!({}), body)).await
}
fn last(
    callbacks: &Callbacks,
    id: &str,
    kind: &str,
) -> Value {
    callbacks
        .messages
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(target, value)| target == id && value["type"] == kind)
        .unwrap()
        .1
        .clone()
}
fn no_secrets(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                assert!(!["seed", "wall", "token_hash", "password_hash"].contains(&key.as_str()), "leaked {key}");
                no_secrets(value);
            }
        },
        Value::Array(values) => {
            for value in values {
                no_secrets(value);
            }
        },
        _ => {},
    }
}

fn fixed_now() -> Result<u64, StoreError> {
    static NOW: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    Ok(*NOW.get_or_init(|| epoch_seconds().unwrap()))
}

#[tokio::test]
#[ignore = "requires DynamoDB Local; run cargo test -p mjers-lambda --test gateway -- --ignored"]
async fn gateway_rooms_connections_and_full_hands() {
    let callbacks = Callbacks::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route("/@connections/{id}", post(callback).delete(close)).with_state(callbacks.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let config = aws_sdk_apigatewaymanagement::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .endpoint_url(endpoint)
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new("local", "local", None, None, "test"))
        .build();
    let database = local::client(&std::env::var("MJERS_DYNAMODB_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8000".into()));
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let tables_name = format!("mjers-gateway-tables-{suffix}");
    let connections_name = format!("mjers-gateway-connections-{suffix}");
    local::ensure_table(&database, &tables_name).await.unwrap();
    local::ensure_connections_table(&database, &connections_name).await.unwrap();
    let tables = TableStore::new(database.clone(), tables_name.clone());
    let connections = ConnectionStore::new(database.clone(), connections_name.clone());
    let limits = RateLimiter::new(database.clone(), connections_name.clone());
    let gateway = Gateway::new(
        RoomService::new(tables.clone()),
        connections.clone(),
        limits.clone(),
        Client::from_conf(config),
        vec!["https://game.example".into(), "https://alias.example".into()],
        fixed_now,
    );

    let created = gateway.handle(http("/tables", "create-host", json!({"name":"Host","password":"family"}))).await;
    assert_eq!(created.status_code, 200, "{}", created.body);
    assert_eq!(created.headers["cache-control"], "no-store");
    let host: Joined = serde_json::from_str(&created.body).unwrap();
    let join_path = format!("/tables/{}/join", host.table);
    let bad = gateway.handle(http(&join_path, "guest", json!({"name":"Guest","password":"wrong"}))).await;
    assert_eq!(bad.status_code, 403);
    let response = gateway.handle(http(&join_path, "guest", json!({"name":"Guest","password":"family"}))).await;
    assert_eq!(response.status_code, 200, "{}", response.body);
    let guest: Joined = serde_json::from_str(&response.body).unwrap();
    let host_query = json!({"table":host.table,"token":host.seat_token});
    let guest_query = json!({"table":guest.table,"token":guest.seat_token});
    let mut forbidden = socket("$connect", "forbidden", host_query.clone(), Value::Null);
    forbidden.headers.as_mut().unwrap().insert("Origin".into(), "https://other.example".into());
    assert_eq!(gateway.handle(forbidden).await.status_code, 403);
    assert_eq!(
        gateway.handle(socket("$connect", "bad", json!({"table":host.table,"token":"wrong"}), Value::Null)).await.status_code,
        403
    );
    assert_eq!(gateway.handle(socket("$connect", "host", host_query.clone(), Value::Null)).await.status_code, 200);
    assert!(callbacks.messages.lock().unwrap().is_empty()); // No premature PostToConnection.
    assert_eq!(message(&gateway, "host", json!({"type":"sync"})).await.status_code, 200);
    assert_eq!(last(&callbacks, "host", "state")["observation"], Value::Null);
    assert_eq!(gateway.handle(socket("$connect", "guest", guest_query, Value::Null)).await.status_code, 200);
    assert_eq!(message(&gateway, "guest", json!({"type":"sync"})).await.status_code, 200);
    assert_eq!(last(&callbacks, "host", "state")["seats"][1]["connected"], true);
    message(&gateway, "host", json!({"type":"start_hand"})).await;
    let state = last(&callbacks, "host", "state");
    let other = last(&callbacks, "guest", "state");
    assert_eq!(state["observation"]["seat"], 0);
    assert_eq!(other["observation"]["seat"], 1);
    assert!(other["observation"]["legal_actions"].as_array().unwrap().is_empty());
    let version = tables.load(&host.table).await.unwrap().unwrap().version();
    message(&gateway, "host", json!({"type":"act","action":"pass","hand_number":1,"steps_seen":0})).await;
    assert_eq!(last(&callbacks, "host", "error")["code"], "illegal_action");
    assert_eq!(tables.load(&host.table).await.unwrap().unwrap().version(), version);
    message(&gateway, "host", json!({"type":"hint","samples":0})).await;
    assert_eq!(last(&callbacks, "host", "hint")["hand_number"], 1);
    let command = json!({"type":"act","action":state["observation"]["legal_actions"][0],"hand_number":1,"steps_seen":0});
    message(&gateway, "host", command.clone()).await;
    message(&gateway, "host", command).await;
    assert_eq!(last(&callbacks, "host", "error")["code"], "stale");

    // Replacement revokes the old mapping before close; late disconnects are harmless.
    let old_connection = connections.load("host", epoch_seconds().unwrap()).await.unwrap().unwrap();
    assert_eq!(gateway.handle(socket("$connect", "host-new", host_query, Value::Null)).await.status_code, 200);
    assert!(callbacks.closed.lock().unwrap().contains(&"host".into()));
    assert!(connections.load("host", epoch_seconds().unwrap()).await.unwrap().is_none());
    let mut candidate = tables.load(&host.table).await.unwrap().unwrap();
    let expected = candidate.version();
    candidate
        .handle(
            Seat::ALL[0],
            ClientMessage::Autoplay {
                enabled: true,
                hand_number: 1,
                steps_seen: candidate.steps(),
            },
            0,
        )
        .unwrap();
    assert!(matches!(
        tables.save_connected(&candidate, expected, &connections.guard(&old_connection)).await,
        Err(StoreError::ConnectionLost)
    ));
    assert_eq!(tables.load(&host.table).await.unwrap().unwrap().version(), expected);
    assert_eq!(gateway.handle(socket("$disconnect", "host", json!({}), Value::Null)).await.status_code, 200);
    assert!(connections.load("host-new", epoch_seconds().unwrap()).await.unwrap().is_some());
    assert_eq!(message(&gateway, "host", json!({"type":"ping"})).await.status_code, 403);
    message(&gateway, "host-new", json!({"type":"sync"})).await;
    let state = last(&callbacks, "host-new", "state");
    message(&gateway, "host-new", json!({"type":"autoplay","enabled":true,"hand_number":1,"steps_seen":state["steps"]})).await;
    message(&gateway, "guest", json!({"type":"sync"})).await;
    let state = last(&callbacks, "guest", "state");
    message(&gateway, "guest", json!({"type":"autoplay","enabled":true,"hand_number":1,"steps_seen":state["steps"]})).await;
    let over = last(&callbacks, "guest", "hand_over");
    assert_eq!(over["revealed"].as_array().unwrap().len(), 4);
    assert_eq!(over["settlement"]["total"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).sum::<i64>(), 0);
    assert!(tables.load_hand(&host.table, 1).await.unwrap().unwrap().replay().unwrap().is_over());
    message(&gateway, "host-new", json!({"type":"next_hand","hand_number":1})).await;
    assert_eq!(last(&callbacks, "host-new", "state")["hand_number"], 2);
    assert_eq!(last(&callbacks, "host-new", "state")["observation"]["dealer"], 1);
    assert!(tables.load_hand(&host.table, 2).await.unwrap().is_some());

    callbacks.gone.lock().unwrap().insert("guest".into());
    message(&gateway, "host-new", json!({"type":"sync"})).await;
    assert!(connections.load("guest", epoch_seconds().unwrap()).await.unwrap().is_none());
    message(&gateway, "host-new", json!({"type":"sync"})).await;
    assert_eq!(last(&callbacks, "host-new", "state")["seats"][1]["connected"], false);
    // Failed password attempts have an atomic room/IP quota.
    for _ in 0..10 {
        assert_eq!(gateway.handle(http(&join_path, "brute-force", json!({"name":"Guest","password":"wrong"}))).await.status_code, 403);
    }
    assert_eq!(gateway.handle(http(&join_path, "brute-force", json!({"name":"Guest","password":"wrong"}))).await.status_code, 429);
    let now = epoch_seconds().unwrap();
    let connection = Connection {
        id: "expired".into(),
        table: host.table.clone(),
        seat: Seat::ALL[3],
        expires_at: now - 1,
        ready: true,
    };
    connections.connect(&connection).await.unwrap();
    assert!(connections.load("expired", now).await.unwrap().is_none());
    assert!(connections.list(&host.table, now).await.unwrap().iter().all(|c| c.id != "expired"));
    // Race independent counter requests. Exactly three are admitted.
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let limiter = limits.clone();
        requests.spawn(async move { limiter.check("atomic-limit", 3, now).await });
    }
    let mut admitted = 0;
    while let Some(result) = requests.join_next().await {
        match result.unwrap() {
            Ok(()) => admitted += 1,
            Err(StoreError::RateLimited) => {},
            Err(error) => panic!("{error}"),
        }
    }
    assert_eq!(admitted, 3);
    limits.check("atomic-limit", 3, now + 60).await.unwrap();
    for (_, value) in callbacks.messages.lock().unwrap().iter() {
        no_secrets(value);
        if value["type"] == "state"
            && let Some(players) = value["observation"]["players"].as_array()
        {
            assert!(players.iter().all(|player| player.get("concealed").is_none()));
        }
        if value["type"] == "events" {
            for event in value["events"].as_array().unwrap() {
                if event["event"] == "draw" {
                    assert!(event.get("tile").is_none());
                }
            }
        }
    }
    for name in [&tables_name, &connections_name] {
        database.delete_table().table_name(name).send().await.unwrap();
    }
    server.abort();
}

#[test]
fn gateway_event_decodes_platform_bodies_without_accepting_oversized_input() {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    let nullable: GatewayEvent = serde_json::from_value(json!({"headers":null,"queryStringParameters":null,"requestContext":{"identity":null},"body":"{}"})).unwrap();
    assert!(nullable.header("origin").is_none());
    assert!(nullable.body().is_ok());
    let event: GatewayEvent = serde_json::from_value(json!({"body":STANDARD.encode("{\"type\":\"sync\"}"),"isBase64Encoded":true})).unwrap();
    assert_eq!(event.body().unwrap(), "{\"type\":\"sync\"}");
    let event: GatewayEvent = serde_json::from_value(json!({"body":"x".repeat(4097)})).unwrap();
    assert!(event.body().is_err());
    let event: GatewayEvent = serde_json::from_value(json!({"body":"!","isBase64Encoded":true})).unwrap();
    assert!(event.body().is_err());
}
