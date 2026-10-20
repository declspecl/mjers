use super::*;

#[test]
fn indices_cover_all_34_kinds_in_order() {
    let tiles: Vec<Tile> = Tile::all().collect();
    assert_eq!(tiles.len(), KINDS);
    for (i, tile) in tiles.iter().enumerate() {
        assert_eq!(tile.index(), i);
    }
}

#[test]
fn suited_tiles_round_trip_suit_and_rank() {
    for suit in Suit::ALL {
        for rank in 1..=9 {
            let tile = Tile::suited(suit, rank).unwrap();
            assert_eq!(tile.suit(), Some(suit));
            assert_eq!(tile.rank(), Some(rank));
            assert_eq!(tile.honor_kind(), None);
        }
    }
}

#[test]
fn honor_tiles_have_no_suit() {
    for honor in Honor::ALL {
        let tile = Tile::honor(honor);
        assert_eq!(tile.honor_kind(), Some(honor));
        assert_eq!(tile.suit(), None);
        assert_eq!(tile.rank(), None);
        assert!(tile.is_honor());
    }
}

#[test]
fn chow_successor_stops_at_suit_boundary() {
    assert_eq!(Tile::suited(Suit::Wan, 8).unwrap().chow_successor(), Some(Tile::suited(Suit::Wan, 9).unwrap()));
    assert_eq!(Tile::suited(Suit::Wan, 9).unwrap().chow_successor(), None);
    assert_eq!(Tile::honor(Honor::East).chow_successor(), None);
}

#[test]
fn display_and_parse_round_trip() {
    for tile in Tile::all() {
        let text = tile.to_string();
        assert_eq!(text.parse::<Tile>(), Ok(tile), "{text}");
    }
    assert_eq!(Tile::suited(Suit::Tong, 5).unwrap().to_string(), "5p");
    assert_eq!(Tile::honor(Honor::Red).to_string(), "7z");
}

#[test]
fn parse_rejects_invalid_tiles() {
    for text in ["", "0m", "8z", "5x", "55m", "m"] {
        assert!(text.parse::<Tile>().is_err(), "{text}");
    }
}

#[test]
fn bonus_tiles_are_distinct() {
    let all: Vec<Bonus> = Bonus::all().collect();
    assert_eq!(all.len(), BONUS_KINDS);
    assert_eq!(all[0], Bonus::Flower(1));
    assert_eq!(all[7], Bonus::Season(4));
}
