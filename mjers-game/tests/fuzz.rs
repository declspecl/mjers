//! Random play across many seeds, checking invariants after every step.

use std::sync::Arc;

use mjers_game::wall::WALL_SIZE;
use mjers_game::{
    Action,
    Game,
    Phase,
    Rng,
    Ruleset,
    Seat,
};
use mjers_hand::Meld;
use mjers_rules::{
    Win,
    WinSource,
};
use mjers_tile::TileCounts;

const GAMES: u64 = 2_000;

fn meld_tiles(game: &Game) -> TileCounts {
    Seat::ALL.iter().flat_map(|&s| game.hand(s).melds()).flat_map(|m: Meld| m.tiles().tiles().collect::<Vec<_>>()).collect()
}

fn check_invariants(game: &Game) {
    let concealed: usize = Seat::ALL.iter().map(|&s| game.hand(s).concealed().len()).sum();
    let bonus: usize = Seat::ALL.iter().map(|&s| game.hand(s).bonus().len() as usize).sum();
    let melds = meld_tiles(game);
    let unclaimed: TileCounts = game.discards().iter().filter(|d| !d.claimed).map(|d| d.tile).collect();
    assert_eq!(concealed + bonus + melds.len() + unclaimed.len() + game.wall().remaining(), WALL_SIZE, "tile conservation");

    let mut public = unclaimed;
    for tile in melds.tiles() {
        public.add(tile);
    }
    // A discard won on stays public though it now completes the winner's hand.
    if let Some(Some(Win { source: WinSource::Discard(_), .. })) = game.result().map(|r| r.win) {
        public.add(game.discards().last().unwrap().tile);
    }
    assert_eq!(&public, game.public_tiles(), "public tiles");

    let acting = match game.phase() {
        Phase::Turn { seat, .. } => Some(*seat),
        _ => None,
    };
    if !game.is_over() {
        for seat in Seat::ALL {
            let expected = if Some(seat) == acting { 14 } else { 13 };
            assert_eq!(game.hand(seat).size(), expected, "{seat} size in {:?}", game.phase());
        }
    }
}

#[test]
fn random_play_preserves_invariants_and_replays() {
    let rules = Arc::new(Ruleset::family());
    let (mut wins, mut kongs, mut claims) = (0, 0, 0);
    for seed in 0..GAMES {
        let mut game = Game::new(rules.clone(), seed, Seat::ALL[(seed % 4) as usize]);
        let mut rng = Rng::seed(seed ^ 0xF00D);
        check_invariants(&game);
        while !game.is_over() {
            let seat = game.to_act()[0];
            let legal = game.legal_actions(seat);
            assert!(!legal.is_empty(), "{seat} asked with no options");
            // Favour claims and kongs so rare paths get exercised.
            let eager: Vec<Action> = legal.iter().copied().filter(|a| !matches!(a, Action::Pass | Action::Discard(_))).collect();
            let action = if !eager.is_empty() && rng.below(2) == 0 {
                eager[rng.below(eager.len() as u64) as usize]
            } else {
                legal[rng.below(legal.len() as u64) as usize]
            };
            match action {
                Action::Win | Action::SelfDrawWin => wins += 1,
                Action::Kong | Action::ConcealedKong(_) | Action::UpgradeKong(_) => kongs += 1,
                Action::Pong | Action::Chow(_) => claims += 1,
                _ => {},
            }
            game.apply(seat, action).unwrap();
            check_invariants(&game);
        }
        let settlement = game.settlement().unwrap();
        assert_eq!(settlement.total().iter().sum::<i32>(), 0);
        if let Some(win) = game.result().unwrap().win {
            assert!(game.hand(win.winner).is_winning(), "seed {seed}: winner holds a complete hand");
        }
        let replayed = game.trace().replay().unwrap();
        assert_eq!(replayed.result(), game.result());
    }
    assert!(wins > 50 && kongs > 50 && claims > 1000, "rare paths exercised: {wins} wins, {kongs} kongs, {claims} claims");
}
