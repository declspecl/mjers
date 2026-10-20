//! A seat's own turn: drawing, discarding, kongs, and self-drawn wins.

use super::{
    Discard,
    Game,
};
use crate::game::action::{
    Action,
    Actions,
    GameError,
    Offer,
    Phase,
};
use crate::game::trace::Event;
use crate::game::wall::WallTile;
use crate::hand::MAX_MELDS;
use crate::hand::meld::Meld;
use crate::rules::seat::Seat;
use crate::rules::settlement::WinSource;
use crate::tile::Tile;
use crate::tile::counts::COPIES;

impl Game {
    pub(super) fn turn_actions(
        &self,
        seat: Seat,
        after_claim: bool,
    ) -> Actions {
        let hand = self.hand(seat);
        let concealed = hand.concealed();
        let mut actions = Actions::new();
        if !after_claim {
            if hand.is_winning() {
                actions.push(Action::SelfDrawWin);
            }
            let can_replace = self.wall.remaining() > 0;
            if can_replace && hand.meld_count() < MAX_MELDS {
                actions.extend(concealed.kinds().filter(|&t| concealed.get(t) == COPIES).map(Action::ConcealedKong));
            }
            if can_replace {
                actions.extend(hand.melds().filter_map(|m| match m {
                    Meld::Pong(t) if concealed.get(t) > 0 => Some(Action::UpgradeKong(t)),
                    _ => None,
                }));
            }
        }
        actions.extend(concealed.kinds().map(Action::Discard));
        actions
    }

    /// Carries out a legal turn action.
    pub(super) fn take_turn(
        &mut self,
        seat: Seat,
        action: Action,
    ) -> Result<(), GameError> {
        match action {
            Action::Discard(tile) => self.discard(seat, tile),
            Action::SelfDrawWin => {
                let tile = self.last_draw.ok_or(GameError::Illegal { seat, action })?;
                self.end_with_win(seat, WinSource::SelfDraw, tile);
                Ok(())
            },
            Action::ConcealedKong(tile) => {
                let meld = self.hand_by_seat[seat.index()].declare_concealed_kong(tile)?;
                self.events.push(Event::Kong { seat, meld });
                for _ in 0..COPIES {
                    self.public.add(tile);
                }
                self.start_turn(seat, true);
                Ok(())
            },
            Action::UpgradeKong(tile) if self.rules.allow_rob_kong => self.open_claims(tile, seat, Offer::KongUpgrade),
            Action::UpgradeKong(tile) => self.complete_upgrade(seat, tile),
            Action::Pass | Action::Chow(_) | Action::Pong | Action::Kong | Action::Win => Err(GameError::Illegal { seat, action }),
        }
    }

    pub(super) fn complete_upgrade(
        &mut self,
        seat: Seat,
        tile: Tile,
    ) -> Result<(), GameError> {
        let meld = self.hand_by_seat[seat.index()].upgrade_kong(tile)?;
        self.events.push(Event::Kong { seat, meld });
        self.public.add(tile);
        self.start_turn(seat, true);
        Ok(())
    }

    fn discard(
        &mut self,
        seat: Seat,
        tile: Tile,
    ) -> Result<(), GameError> {
        self.hand_by_seat[seat.index()].discard(tile)?;
        self.discards.push(Discard { seat, tile, claimed: false });
        self.events.push(Event::Discard { seat, tile });
        self.public.add(tile);
        self.open_claims(tile, seat, Offer::Discard)
    }

    /// Begins `seat`'s turn with a live draw, or a replacement draw after a kong.
    /// Ends the hand if the wall cannot supply the tile.
    pub(super) fn start_turn(
        &mut self,
        seat: Seat,
        replacement: bool,
    ) {
        self.last_draw = None;
        let next = if replacement { self.wall.draw_replacement() } else { self.wall.draw() };
        let Some(next) = next else {
            self.end_exhausted();
            return;
        };
        if let Some(drawn) = self.receive(seat, next) {
            self.events.push(Event::Draw { seat, tile: drawn });
            self.last_draw = Some(drawn);
            self.phase = Phase::Turn { seat, after_claim: false };
        }
    }

    /// Adds a tile to `seat`'s hand, setting flowers aside and drawing
    /// replacements until a playable tile arrives. Ends the hand if the wall
    /// runs out first.
    pub(super) fn receive(
        &mut self,
        seat: Seat,
        mut tile: WallTile,
    ) -> Option<Tile> {
        loop {
            match tile {
                WallTile::Tile(t) => {
                    self.hand_by_seat[seat.index()].add_tile(t);
                    return Some(t);
                },
                WallTile::Bonus(bonus) => {
                    self.hand_by_seat[seat.index()].add_bonus(bonus);
                    self.events.push(Event::Bonus { seat, bonus });
                    match self.wall.draw_replacement() {
                        Some(next) => tile = next,
                        None => {
                            self.end_exhausted();
                            return None;
                        },
                    }
                },
            }
        }
    }
}
