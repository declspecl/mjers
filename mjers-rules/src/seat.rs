//! Table positions in turn order.

use std::fmt;

use serde::{
    Deserialize,
    Serialize,
};

pub const SEATS: usize = 4;

/// A table position. Play passes counter-clockwise, which is `next()`; the
/// player on your left is `previous()`, the only one whose discards you may chow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Seat(u8);

impl Seat {
    pub const ALL: [Seat; SEATS] = [Seat(0), Seat(1), Seat(2), Seat(3)];

    pub const fn new(index: usize) -> Option<Seat> {
        if index < SEATS { Some(Seat(index as u8)) } else { None }
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn next(self) -> Seat {
        Seat((self.0 + 1) % SEATS as u8)
    }

    pub const fn previous(self) -> Seat {
        Seat((self.0 + SEATS as u8 - 1) % SEATS as u8)
    }

    /// How many turns after `from` this seat acts (0 for `from` itself).
    pub const fn turns_after(
        self,
        from: Seat,
    ) -> u8 {
        (self.0 + SEATS as u8 - from.0) % SEATS as u8
    }

    /// The other seats in turn order starting after this one.
    pub fn others(self) -> impl Iterator<Item = Seat> {
        (1..SEATS as u8).map(move |k| Seat((self.0 + k) % SEATS as u8))
    }
}

impl fmt::Display for Seat {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "seat {}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_order_wraps() {
        assert_eq!(Seat::ALL[3].next(), Seat::ALL[0]);
        assert_eq!(Seat::ALL[0].previous(), Seat::ALL[3]);
        for seat in Seat::ALL {
            assert_eq!(seat.next().previous(), seat);
        }
    }

    #[test]
    fn turns_after_counts_forward() {
        assert_eq!(Seat::ALL[1].turns_after(Seat::ALL[0]), 1);
        assert_eq!(Seat::ALL[0].turns_after(Seat::ALL[1]), 3);
        assert_eq!(Seat::ALL[2].turns_after(Seat::ALL[2]), 0);
    }

    #[test]
    fn others_in_turn_order() {
        assert_eq!(Seat::ALL[2].others().collect::<Vec<_>>(), [Seat::ALL[3], Seat::ALL[0], Seat::ALL[1]]);
    }
}
