//! These tests execute real DynamoDB requests, never a mocked SDK client.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{
    SystemTime,
    UNIX_EPOCH,
};

use aws_sdk_dynamodb::types::AttributeValue;
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game;
use mjers_core::game::action::Action;
use mjers_core::rules::Ruleset;
use mjers_core::rules::seat::Seat;
use mjers_dynamodb::{
    StoreError,
    TableStore,
    local,
};
use mjers_table::protocol::{
    ClientMessage,
    ErrorCode,
};
use mjers_table::{
    BotPreset,
    Table,
};

fn room(
    id: &str,
    humans: u8,
) -> Table {
    Table::new(id.into(), "Host".into(), "host-token", humans, None, BotPreset::Efficiency).unwrap()
}
fn equal(
    left: &Table,
    right: &Table,
) {
    assert_eq!(serde_json::to_value(left).unwrap(), serde_json::to_value(right).unwrap());
}
fn act(
    table: &Table,
    action: Action,
) -> ClientMessage {
    ClientMessage::Act {
        action,
        hand_number: table.hand_number(),
        steps_seen: table.steps(),
    }
}

#[tokio::test]
#[ignore = "requires DynamoDB Local; run cargo test -p mjers-dynamodb --test local -- --ignored"]
async fn dynamodb_persistence_and_conditional_writes() {
    let endpoint = std::env::var("MJERS_DYNAMODB_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:8000".into());
    let client = local::client(&endpoint);
    let name = format!("mjers-test-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
    local::ensure_table(&client, &name).await.unwrap();
    local::ensure_table(&client, &name).await.unwrap();
    let store = TableStore::new(client.clone(), name.clone());
    assert!(store.load("missing").await.unwrap().is_none());

    let mut original = room("roundtrip", 1);
    store.create(&original).await.unwrap();
    assert!(matches!(store.create(&original).await, Err(StoreError::Conflict)));
    equal(&store.load(original.id()).await.unwrap().unwrap(), &original);
    original.handle(Seat::ALL[0], ClientMessage::StartHand, u64::MAX).unwrap();
    store.save(&original, 0).await.unwrap();
    let loaded = store.load(original.id()).await.unwrap().unwrap();
    equal(&loaded, &original); // Includes full-width u64 seed, actions and credentials.
    assert_eq!(loaded.authenticate("host-token").unwrap(), Seat::ALL[0]);
    assert!(matches!(store.save(&original, 0).await, Err(StoreError::Conflict)));
    assert!(matches!(store.save(&original, 1).await, Err(StoreError::InvalidRecord(_))));
    // Saving a deleted/nonexistent item cannot resurrect it via a CAS write.
    let mut missing = room("not-created", 1);
    missing.handle(Seat::ALL[0], ClientMessage::StartHand, 1).unwrap();
    assert!(matches!(store.save(&missing, 0).await, Err(StoreError::Conflict)));

    // Separate store instances race for the same last human seat. Exactly one
    // commit wins; retrying the loser against a fresh read reports table_full.
    let two = room("join-race", 2);
    store.create(&two).await.unwrap();
    let other = TableStore::new(client.clone(), name.clone());
    let mut a = store.load(two.id()).await.unwrap().unwrap();
    let mut b = other.load(two.id()).await.unwrap().unwrap();
    a.join("A".into(), "token-a", None).unwrap();
    b.join("B".into(), "token-b", None).unwrap();
    let (first, second) = tokio::join!(store.save(&a, 0), other.save(&b, 0));
    assert_ne!(first.is_ok(), second.is_ok());
    assert!(matches!(if first.is_ok() { second } else { first }, Err(StoreError::Conflict)));
    let mut current = store.load(two.id()).await.unwrap().unwrap();
    assert_eq!(current.version(), 1);
    assert_eq!(current.join("C".into(), "token-c", None).unwrap_err().code, ErrorCode::TableFull);

    // Reach a real engine claim window, then race two human replies at the
    // storage boundary. Re-evaluation rejects the old steps_seen; a refreshed
    // response commits and the replay agrees with the engine's resolution.
    let (mut table, mut game) = claim_window();
    store.create(&table).await.unwrap();
    let asked: Vec<_> = game.to_act().iter().collect();
    let base = table.version();
    let mut first = store.load(table.id()).await.unwrap().unwrap();
    let mut second = other.load(table.id()).await.unwrap().unwrap();
    let second_message = act(&table, Action::Pass);
    first.handle(asked[0], act(&table, Action::Pass), 0).unwrap();
    second.handle(asked[1], second_message.clone(), 0).unwrap();
    let (a, b) = tokio::join!(store.save(&first, base), other.save(&second, base));
    assert_ne!(a.is_ok(), b.is_ok());
    let (winner, loser) = if a.is_ok() { (asked[0], asked[1]) } else { (asked[1], asked[0]) };
    game.apply(winner, Action::Pass).unwrap();
    table = store.load(table.id()).await.unwrap().unwrap();
    assert_eq!(table.handle(loser, second_message, 0).err().unwrap().code, ErrorCode::Stale);
    let version = table.version();
    table.handle(loser, act(&table, Action::Pass), 0).unwrap();
    game.apply(loser, Action::Pass).unwrap();
    store.save(&table, version).await.unwrap();
    let loaded = store.load(table.id()).await.unwrap().unwrap();
    for seat in Seat::ALL {
        let messages = serde_json::to_value(loaded.messages(seat, [false; 4]).unwrap()).unwrap();
        assert_eq!(messages[0]["observation"], serde_json::to_value(mjers_core::game::snapshot::Snapshot::new(&game, seat)).unwrap());
    }

    // Read failures and malformed data never turn into an empty/default room.
    client
        .update_item()
        .table_name(&name)
        .key("table_id", AttributeValue::S("roundtrip".into()))
        .update_expression("SET #v = :v")
        .expression_attribute_names("#v", "version")
        .expression_attribute_values(":v", AttributeValue::N("999".into()))
        .send()
        .await
        .unwrap();
    assert!(matches!(store.load("roundtrip").await, Err(StoreError::InvalidRecord(_))));
    client.delete_table().table_name(&name).send().await.unwrap();
    assert!(store.load("join-race").await.is_err());
}

fn claim_window() -> (Table, Game) {
    for seed in 0..100 {
        let mut table = room("claim-race", 4);
        for n in 1..4 {
            table.join(format!("Player {n}"), &format!("token-{n}"), None).unwrap();
        }
        table.handle(Seat::ALL[0], ClientMessage::StartHand, seed).unwrap();
        let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]).unwrap();
        let spec = BotSpec::preset("efficiency").unwrap();
        while !game.is_over() {
            if game.to_act().len() >= 2 {
                return (table, game);
            }
            let seat = game.to_act().first().unwrap();
            let observation = game.observe(seat);
            let chosen = spec.build(0).act(&observation, &observation.legal_actions());
            table.handle(seat, act(&table, chosen), 0).unwrap();
            game.apply(seat, chosen).unwrap();
        }
    }
    panic!("no multi-claim window found");
}
