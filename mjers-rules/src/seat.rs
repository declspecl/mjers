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

/// A set of seats, such as those that must decide before play continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SeatSet(u8);

impl SeatSet {
    pub const EMPTY: SeatSet = SeatSet(0);

    pub const fn only(seat: Seat) -> SeatSet {
        SeatSet(1 << seat.0)
    }

    pub fn insert(
        &mut self,
        seat: Seat,
    ) {
        self.0 |= 1 << seat.0;
    }

    pub const fn contains(
        self,
        seat: Seat,
    ) -> bool {
        self.0 & (1 << seat.0) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The lowest-numbered seat in the set.
    pub fn first(self) -> Option<Seat> {
        self.iter().next()
    }

    pub const fn iter(self) -> SeatSetIter {
        SeatSetIter(self.0)
    }
}

impl IntoIterator for SeatSet {
    type IntoIter = SeatSetIter;
    type Item = Seat;

    fn into_iter(self) -> SeatSetIter {
        self.iter()
    }
}

/// Seats of a `SeatSet` in index order, read straight from its bits.
#[derive(Debug, Clone)]
pub struct SeatSetIter(u8);

impl Iterator for SeatSetIter {
    type Item = Seat;

    fn next(&mut self) -> Option<Seat> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as u8;
        self.0 &= self.0 - 1;
        Some(Seat(index))
    }
}

impl FromIterator<Seat> for SeatSet {
    fn from_iter<I: IntoIterator<Item = Seat>>(seats: I) -> Self {
        let mut set = SeatSet::EMPTY;
        for seat in seats {
            set.insert(seat);
        }
        set
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
    fn seat_sets_track_membership() {
        let set: SeatSet = [Seat::ALL[3], Seat::ALL[1]].into_iter().collect();
        assert_eq!(set.len(), 2);
        assert!(set.contains(Seat::ALL[1]) && !set.contains(Seat::ALL[0]));
        assert_eq!(set.first(), Some(Seat::ALL[1]));
        assert_eq!(set.iter().collect::<Vec<_>>(), [Seat::ALL[1], Seat::ALL[3]]);
        assert!(SeatSet::EMPTY.is_empty() && SeatSet::EMPTY.first().is_none());
        assert_eq!(SeatSet::only(Seat::ALL[2]).first(), Some(Seat::ALL[2]));
    }

    #[test]
    fn others_in_turn_order() {
        assert_eq!(Seat::ALL[2].others().collect::<Vec<_>>(), [Seat::ALL[3], Seat::ALL[0], Seat::ALL[1]]);
    }
}
