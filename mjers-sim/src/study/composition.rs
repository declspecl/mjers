//! What winning hands are made of: exposed vs concealed sets, chow suits and
//! positions, and runs of chows.

use mjers_game::game::Game;
use mjers_game::trace::Event;
use mjers_hand::meld::{
    KongKind,
    Meld,
};
use mjers_rules::seat::Seat;
use mjers_tile::tile::Tile;
use serde::{
    Deserialize,
    Serialize,
};

/// Totals over winning hands; divide by `hands` for averages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Composition {
    pub hands: u64,
    pub exposed_chows: u64,
    pub concealed_chows: u64,
    pub exposed_pongs: u64,
    pub concealed_pongs: u64,
    pub exposed_kongs: u64,
    pub concealed_kongs: u64,
    /// Chows touching a terminal (123 or 789) and the rest.
    pub terminal_chows: u64,
    pub inner_chows: u64,
    /// Exposed chows by the wait the claimed tile completed.
    pub claimed_two_sided: u64,
    pub claimed_gap: u64,
    pub claimed_edge: u64,
    /// Hands with two or more chows...
    pub multi_chow_hands: u64,
    /// ...by how many suits those chows span (index 1-3).
    pub chow_suits: [u64; 4],
    /// ...containing the same chow twice (123 + 123).
    pub repeated: u64,
    /// ...containing overlapping chows in one suit (345 + 456): 4-5 in a row.
    pub overlapping: u64,
    /// ...containing back-to-back chows in one suit (456 + 789): 6 in a row.
    pub back_to_back: u64,
    /// ...containing overlapping or back-to-back chows: 4+ in a row.
    pub run_of_four_plus: u64,
    /// ...containing 123 + 456 + 789 in one suit.
    pub straight: u64,
}

impl Composition {
    pub fn merge(
        &mut self,
        o: &Composition,
    ) {
        let pairs = [
            (&mut self.hands, o.hands),
            (&mut self.exposed_chows, o.exposed_chows),
            (&mut self.concealed_chows, o.concealed_chows),
            (&mut self.exposed_pongs, o.exposed_pongs),
            (&mut self.concealed_pongs, o.concealed_pongs),
            (&mut self.exposed_kongs, o.exposed_kongs),
            (&mut self.concealed_kongs, o.concealed_kongs),
            (&mut self.terminal_chows, o.terminal_chows),
            (&mut self.inner_chows, o.inner_chows),
            (&mut self.claimed_two_sided, o.claimed_two_sided),
            (&mut self.claimed_gap, o.claimed_gap),
            (&mut self.claimed_edge, o.claimed_edge),
            (&mut self.multi_chow_hands, o.multi_chow_hands),
            (&mut self.repeated, o.repeated),
            (&mut self.overlapping, o.overlapping),
            (&mut self.back_to_back, o.back_to_back),
            (&mut self.run_of_four_plus, o.run_of_four_plus),
            (&mut self.straight, o.straight),
        ];
        for (mine, theirs) in pairs {
            *mine += theirs;
        }
        for (mine, theirs) in self.chow_suits.iter_mut().zip(o.chow_suits) {
            *mine += theirs;
        }
    }

    /// Records the winner's hand: `concealed_sets` is its decomposed concealed part.
    pub fn record(
        &mut self,
        game: &Game,
        winner: Seat,
        concealed_sets: &[Meld],
    ) {
        self.hands += 1;
        let exposed: Vec<Meld> = game.hand(winner).melds().collect();
        for meld in &exposed {
            match meld {
                Meld::Chow(_) => self.exposed_chows += 1,
                Meld::Pong(_) => self.exposed_pongs += 1,
                Meld::Kong(_, KongKind::Exposed) => self.exposed_kongs += 1,
                Meld::Kong(_, KongKind::Concealed) => self.concealed_kongs += 1,
            }
        }
        for meld in concealed_sets {
            match meld {
                Meld::Chow(_) => self.concealed_chows += 1,
                _ => self.concealed_pongs += 1,
            }
        }

        let chows: Vec<Tile> = exposed.iter().chain(concealed_sets).filter_map(|m| if let Meld::Chow(low) = m { Some(*low) } else { None }).collect();
        for low in &chows {
            match low.rank() {
                Some(1 | 7) => self.terminal_chows += 1,
                _ => self.inner_chows += 1,
            }
        }
        self.record_claimed_chows(game, winner);
        if chows.len() < 2 {
            return;
        }
        self.multi_chow_hands += 1;
        let mut suits: Vec<_> = chows.iter().filter_map(|t| t.suit()).collect();
        suits.sort();
        suits.dedup();
        self.chow_suits[suits.len()] += 1;

        let mut flags = [false; 4];
        for (i, a) in chows.iter().enumerate() {
            for b in &chows[i + 1..] {
                if a.suit() != b.suit() {
                    continue;
                }
                match a.index().abs_diff(b.index()) {
                    0 => flags[0] = true,
                    1 | 2 => flags[1] = true,
                    3 => flags[2] = true,
                    _ => {},
                }
            }
        }
        let straight = chows
            .iter()
            .any(|a| a.rank() == Some(1) && [3, 6].iter().all(|&step| chows.iter().any(|b| b.suit() == a.suit() && b.index() == a.index() + step)));
        flags[3] = straight;
        self.repeated += flags[0] as u64;
        self.overlapping += flags[1] as u64;
        self.back_to_back += flags[2] as u64;
        self.run_of_four_plus += (flags[1] || flags[2]) as u64;
        self.straight += flags[3] as u64;
    }

    /// Classifies each chow the winner claimed by where the claimed tile sat.
    fn record_claimed_chows(
        &mut self,
        game: &Game,
        winner: Seat,
    ) {
        let mut last_discard = None;
        for event in game.events() {
            match *event {
                Event::Discard { tile, .. } => last_discard = Some(tile),
                Event::Claim { seat, meld: Meld::Chow(low), .. } if seat == winner => {
                    let claimed = last_discard.expect("claims follow discards");
                    let rank = low.rank().expect("chows are suited");
                    match claimed.index() - low.index() {
                        1 => self.claimed_gap += 1,
                        2 if rank == 1 => self.claimed_edge += 1,
                        0 if rank == 7 => self.claimed_edge += 1,
                        _ => self.claimed_two_sided += 1,
                    }
                },
                _ => {},
            }
        }
    }
}
