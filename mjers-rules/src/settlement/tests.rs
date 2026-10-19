use super::*;
use crate::ruleset::Ruleset;

const S: [Seat; SEATS] = Seat::ALL;

fn family() -> Payouts {
    Ruleset::family().payouts
}

fn win(
    winner: Seat,
    source: WinSource,
    bonus_tiles: u8,
) -> HandResult {
    HandResult {
        win: Some(Win { winner, source, bonus_tiles }),
        kongs: Vec::new(),
    }
}

#[test]
fn discard_win_charges_the_discarder_double() {
    let s = family().settle(&win(S[0], WinSource::Discard(S[2]), 0));
    assert_eq!(s.win, [4, -1, -2, -1]);
    assert_eq!(s.kongs, [0; SEATS]);
}

#[test]
fn self_draw_charges_everyone_three() {
    let s = family().settle(&win(S[1], WinSource::SelfDraw, 0));
    assert_eq!(s.win, [-3, 9, -3, -3]);
}

#[test]
fn bonus_tiles_add_one_per_payer() {
    let s = family().settle(&win(S[1], WinSource::SelfDraw, 2));
    assert_eq!(s.win, [-5, 15, -5, -5]);
    let s = family().settle(&win(S[0], WinSource::Discard(S[3]), 1));
    assert_eq!(s.win, [7, -2, -2, -3]);
}

#[test]
fn kongs_pay_per_kind_from_every_other_seat() {
    let mut result = win(S[0], WinSource::SelfDraw, 0);
    result.kongs = vec![
        Kong { owner: S[1], kind: KongKind::Exposed },
        Kong {
            owner: S[2],
            kind: KongKind::Concealed,
        },
    ];
    let s = family().settle(&result);
    assert_eq!(s.kongs, [-3, 1, 5, -3]);
    assert_eq!(s.total(), [6, -2, 2, -6]);
}

#[test]
fn kongs_paid_on_exhausted_wall_only_when_configured() {
    let result = HandResult {
        win: None,
        kongs: vec![Kong {
            owner: S[3],
            kind: KongKind::Concealed,
        }],
    };
    assert_eq!(family().settle(&result).total(), [-2, -2, -2, 6]);
    let unpaid = Payouts {
        kongs_paid_without_winner: false,
        ..family()
    };
    assert_eq!(unpaid.settle(&result).total(), [0; SEATS]);
}

#[test]
fn discarder_pays_variant_spares_bystanders() {
    let payouts = Ruleset::discarder_pays().payouts;
    let s = payouts.settle(&win(S[0], WinSource::Discard(S[1]), 1));
    assert_eq!(s.win, [5, -5, 0, 0]);
}

#[test]
fn settlements_are_zero_sum() {
    for rules in Ruleset::PRESETS.map(|n| Ruleset::preset(n).unwrap()) {
        for winner in S {
            for source in [WinSource::SelfDraw, WinSource::Discard(winner.next()), WinSource::Discard(winner.previous())] {
                let mut result = win(winner, source, 3);
                result.kongs = vec![Kong {
                    owner: winner.next(),
                    kind: KongKind::Concealed,
                }];
                let s = rules.payouts.settle(&result);
                assert_eq!(s.win.iter().sum::<i32>(), 0);
                assert_eq!(s.kongs.iter().sum::<i32>(), 0);
            }
        }
    }
}

#[test]
fn hand_result_json_round_trip() {
    let mut result = win(S[0], WinSource::Discard(S[1]), 2);
    result.kongs = vec![Kong {
        owner: S[2],
        kind: KongKind::Concealed,
    }];
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"concealed\""), "{json}");
    assert_eq!(serde_json::from_str::<HandResult>(&json).unwrap(), result);
}
