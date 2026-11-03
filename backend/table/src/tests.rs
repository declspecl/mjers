#![allow(clippy::unwrap_used, clippy::panic)]

use argon2::PasswordHasher;
use argon2::password_hash::SaltString;
use serde_json::{
    Value,
    json,
};

use super::*;

fn room(humans: u8) -> Table {
    Table::new("ABC123".into(), "Host".into(), "host-secret", humans, None, BotPreset::Efficiency).unwrap()
}
fn start(
    table: &mut Table,
    seed: u64,
) {
    table.handle(Seat::ALL[0], ClientMessage::StartHand, seed).unwrap();
}
fn action(
    table: &Table,
    _seat: Seat,
    action: mjers_core::game::action::Action,
) -> ClientMessage {
    ClientMessage::Act {
        action,
        hand_number: table.hand_number(),
        steps_seen: table.steps(),
    }
}
fn finish(table: &mut Table) {
    for _ in 0..1000 {
        let game = table.game().unwrap();
        if game.is_over() {
            return;
        }
        let seat = game.to_act().first().unwrap();
        let obs = game.observe(seat);
        let chosen = BotPreset::Efficiency.spec().build(0).act(&obs, &obs.legal_actions());
        table.handle(seat, action(table, seat, chosen), 0).unwrap();
    }
    panic!("hand did not finish");
}

#[test]
fn lobby_reserves_humans_and_authenticates_named_players() {
    let mut table = room(2);
    assert_eq!(table.authenticate("host-secret").unwrap(), Seat::ALL[0]);
    assert_eq!(table.authenticate("wrong").unwrap_err().code, ErrorCode::BadToken);
    assert_eq!(table.handle(Seat::ALL[0], ClientMessage::StartHand, 0).err().unwrap().code, ErrorCode::NotReady);
    assert!(table.hand.is_none());
    let seat = table.join("  家人  ".into(), "friend-secret", None).unwrap();
    assert_eq!(seat, Seat::ALL[1]);
    assert_eq!(table.authenticate("friend-secret").unwrap(), seat);
    assert_eq!(table.join("Extra".into(), "extra", None).unwrap_err().code, ErrorCode::TableFull);
    assert_eq!(table.handle(seat, ClientMessage::StartHand, 0).err().unwrap().code, ErrorCode::NotHost);
    let state = serde_json::to_value(&table.messages(seat, [false; 4]).unwrap()[0]).unwrap();
    assert_eq!(state["seats"][1]["name"], "家人");
    assert_eq!(state["seats"][2]["kind"], "bot");
    assert_eq!(state["observation"], Value::Null);
    start(&mut table, 7);
    assert_eq!(table.join("Late".into(), "late", None).unwrap_err().code, ErrorCode::HandInProgress);
}

#[test]
fn room_password_is_required_and_never_sent() {
    let salt = SaltString::encode_b64(&[7; 16]).unwrap();
    let hash = Argon2::default().hash_password(b"family", &salt).unwrap().to_string();
    let mut table = Table::new("ABC123".into(), "Host".into(), "secret", 2, Some(hash), BotPreset::Efficiency).unwrap();
    let before = serde_json::to_value(&table).unwrap();
    for password in [None, Some("wrong")] {
        assert_eq!(table.join("Friend".into(), "friend", password).unwrap_err().code, ErrorCode::BadPassword);
        assert_eq!(serde_json::to_value(&table).unwrap(), before);
    }
    table.join("Friend".into(), "friend", Some("family")).unwrap();
    let text = serde_json::to_string(&table.messages(Seat::ALL[0], [false; 4]).unwrap()).unwrap();
    assert!(!text.contains("argon2") && !text.contains("secret") && !text.contains("token_hash"));
}

#[test]
fn invalid_configuration_and_names_are_rejected() {
    for count in [0, 5, 255] {
        assert!(Table::new("A".into(), "Host".into(), "token", count, None, BotPreset::Efficiency).is_err());
    }
    for name in ["".into(), "  ".into(), "x".repeat(41)] {
        assert!(Table::new("A".into(), name, "token", 2, None, BotPreset::Efficiency).is_err());
    }
}

#[test]
fn illegal_stale_and_duplicate_actions_change_nothing() {
    let mut table = room(1);
    start(&mut table, 42);
    let before = serde_json::to_value(&table).unwrap();
    let error = table.handle(Seat::ALL[0], action(&table, Seat::ALL[0], mjers_core::game::action::Action::Pass), 0).err().unwrap();
    assert_eq!(error.code, ErrorCode::IllegalAction);
    assert_eq!(serde_json::to_value(&table).unwrap(), before);
    let game = table.game().unwrap();
    let chosen = game.legal_actions(Seat::ALL[0])[0];
    let command = action(&table, Seat::ALL[0], chosen);
    table.handle(Seat::ALL[0], command.clone(), 0).unwrap();
    let after = serde_json::to_value(&table).unwrap();
    assert_eq!(table.handle(Seat::ALL[0], command, 0).err().unwrap().code, ErrorCode::Stale);
    assert_eq!(serde_json::to_value(&table).unwrap(), after);
}

#[test]
fn series_rotates_dealer_preserves_history_and_settles_once() {
    let mut table = room(1);
    let mut expected = [0; 4];
    for number in 1..=10 {
        if number == 1 {
            start(&mut table, 10);
        } else {
            table.handle(Seat::ALL[0], ClientMessage::NextHand { hand_number: number - 1 }, 10 + number as u64).unwrap();
        }
        assert_eq!(table.hand_number(), number);
        assert_eq!(table.game().unwrap().dealer(), Seat::ALL[(number as usize - 1) % 4]);
        finish(&mut table);
        for (score, delta) in expected.iter_mut().zip(table.game().unwrap().settlement().unwrap().total()) {
            *score += delta;
        }
        assert!(table.history.len() <= 4);
        assert_eq!(table.scores, expected);
        assert_eq!(table.scores.iter().sum::<i32>(), 0);
        assert_eq!(table.messages(Seat::ALL[0], [false; 4]).unwrap().len(), 2);
    }
    let previous = table.hand_number();
    table.handle(Seat::ALL[0], ClientMessage::NextHand { hand_number: previous }, 87).unwrap();
    let before = serde_json::to_value(&table).unwrap();
    assert_eq!(
        table
            .handle(
                Seat::ALL[0],
                ClientMessage::Act {
                    action: mjers_core::game::action::Action::Pass,
                    hand_number: previous,
                    steps_seen: 0
                },
                0
            )
            .err()
            .unwrap()
            .code,
        ErrorCode::Stale
    );
    assert_eq!(serde_json::to_value(&table).unwrap(), before);
}

fn assert_public(value: &Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                assert!(!["seed", "wall", "token_hash", "password_hash", "steps_log"].contains(&key.as_str()), "leaked {key}");
                assert_public(value);
            }
        },
        Value::Array(values) => {
            for value in values {
                assert_public(value);
            }
        },
        _ => {},
    }
}

#[test]
fn two_humans_finish_hands_with_safe_per_seat_messages_and_events() {
    for seed in 0..30 {
        let mut table = room(2);
        table.join("Friend".into(), "friend", None).unwrap();
        start(&mut table, seed);
        for _ in 0..1000 {
            let game = table.game().unwrap();
            for seat in [Seat::ALL[0], Seat::ALL[1]] {
                let messages = table.messages(seat, [true, true, false, false]).unwrap();
                let state = serde_json::to_value(&messages[0]).unwrap();
                assert_public(&state);
                assert_eq!(state["observation"], serde_json::to_value(Snapshot::new(&game, seat)).unwrap());
                let players = state["observation"]["players"].as_array().unwrap();
                for player in players {
                    assert!(player.get("concealed").is_none());
                }
            }
            if game.is_over() {
                break;
            }
            assert!(game.to_act().iter().all(|seat| matches!(table.seats[seat.index()], SeatSlot::Human { .. })));
            let seat = game.to_act().first().unwrap();
            let obs = game.observe(seat);
            let chosen = BotPreset::Efficiency.spec().build(0).act(&obs, &obs.legal_actions());
            let change = table.handle(seat, action(&table, seat, chosen), 0).unwrap();
            let events = serde_json::to_value(ServerMessage::Events {
                version: table.version(),
                hand_number: table.hand_number(),
                events: change.events,
            })
            .unwrap();
            assert_public(&events);
            for event in events["events"].as_array().unwrap() {
                if event["event"] == "draw" {
                    assert!(event.get("tile").is_none());
                }
                assert_ne!(event["event"], "deal");
            }
        }
        assert!(table.game().unwrap().is_over());
    }
}

#[test]
fn two_human_claims_reject_stale_then_resolve_after_refresh() {
    let mut found = false;
    for seed in 0..100 {
        let mut table = room(4);
        for n in 1..4 {
            table.join(format!("Player {n}"), &format!("token-{n}"), None).unwrap();
        }
        start(&mut table, seed);
        for _ in 0..1000 {
            let game = table.game().unwrap();
            if game.is_over() {
                break;
            }
            let asked: Vec<_> = game.to_act().iter().collect();
            if asked.len() >= 2 {
                let pass = mjers_core::game::action::Action::Pass;
                let second = action(&table, asked[1], pass);
                table.handle(asked[0], action(&table, asked[0], pass), 0).unwrap();
                let before = serde_json::to_value(&table).unwrap();
                assert_eq!(table.handle(asked[1], second, 0).err().unwrap().code, ErrorCode::Stale);
                assert_eq!(serde_json::to_value(&table).unwrap(), before);
                table.handle(asked[1], action(&table, asked[1], pass), 0).unwrap();
                finish(&mut table);
                found = true;
                break;
            }
            let seat = asked[0];
            let obs = game.observe(seat);
            let chosen = BotPreset::Efficiency.spec().build(0).act(&obs, &obs.legal_actions());
            table.handle(seat, action(&table, seat, chosen), 0).unwrap();
        }
        if found {
            break;
        }
    }
    assert!(found);
}

#[test]
fn two_hundred_bot_hands_replay_without_consulting_bots() {
    for seed in 0..200 {
        let mut table = room(1);
        table.seats = std::array::from_fn(|_| SeatSlot::Bot {
            preset: if seed % 2 == 0 { BotPreset::Efficiency } else { BotPreset::Random },
        });
        let mut game = Game::new(Arc::new(Ruleset::family()), seed, Seat::ALL[0]).unwrap();
        table.advance_bots(&mut game).unwrap();
        assert!(game.is_over());
        table.save_game(&game);
        let persisted = serde_json::to_string(&table).unwrap();
        let loaded: Table = serde_json::from_str(&persisted).unwrap();
        let replayed = loaded.game().unwrap();
        assert_eq!(replayed.trace(), game.trace());
        for seat in Seat::ALL {
            assert_eq!(Snapshot::new(&replayed, seat), Snapshot::new(&game, seat));
        }
    }
}

#[test]
fn protocol_examples_round_trip_and_hints_do_not_mutate() {
    for value in [
        json!({"type":"start_hand"}),
        json!({"type":"act", "action":{"discard":"5m"}, "hand_number":1,"steps_seen":3}),
        json!({"type":"next_hand","hand_number":1}),
        json!({"type":"hint","samples":0}),
        json!({"type":"ping"}),
    ] {
        let parsed: ClientMessage = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    assert!(serde_json::from_value::<ClientMessage>(json!({"type":"act","seat":2,"action":"pass","hand_number":1,"steps_seen":0})).is_err());
    assert_eq!(serde_json::to_value(ServerMessage::Pong).unwrap(), json!({"type":"pong"}));
    let mut table = room(1);
    start(&mut table, 4);
    let before = serde_json::to_value(&table).unwrap();
    let hint = table.read(Seat::ALL[0], ClientMessage::Hint { samples: 0 }).unwrap();
    assert_eq!(serde_json::to_value(hint).unwrap()["type"], "hint");
    assert_eq!(serde_json::to_value(&table).unwrap(), before);
    assert!(table.read(Seat::ALL[0], ClientMessage::Hint { samples: 200 }).is_err());
}

#[test]
fn autoplay_keeps_the_human_identity_and_does_not_double_settle() {
    let mut table = room(1);
    start(&mut table, 7);
    table
        .handle(
            Seat::ALL[0],
            ClientMessage::Autoplay {
                enabled: true,
                hand_number: 1,
                steps_seen: 0,
            },
            0,
        )
        .unwrap();
    assert!(table.game().unwrap().is_over());
    let scores = table.scores;
    assert_eq!(table.authenticate("host-secret").unwrap(), Seat::ALL[0]);
    table
        .handle(
            Seat::ALL[0],
            ClientMessage::Autoplay {
                enabled: false,
                hand_number: 1,
                steps_seen: table.steps(),
            },
            0,
        )
        .unwrap();
    assert_eq!(table.scores, scores);
    table.handle(Seat::ALL[0], ClientMessage::NextHand { hand_number: 1 }, 99).unwrap();
    assert!(!table.game().unwrap().is_over());
}
