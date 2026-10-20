use super::*;
use crate::tile::Suit;

#[test]
fn parses_grouped_notation() {
    let counts: TileCounts = "123m55p7z".parse().unwrap();
    assert_eq!(counts.len(), 6);
    assert_eq!(counts.get(Tile::suited(Suit::Wan, 1).unwrap()), 1);
    assert_eq!(counts.get(Tile::suited(Suit::Tong, 5).unwrap()), 2);
    assert_eq!(counts.get(Tile::honor(Honor::Red)), 1);
}

#[test]
fn parse_rejects_more_than_four_copies() {
    assert_eq!("11111m".parse::<TileCounts>(), Err(ParseError::TooManyCopies(Tile::suited(Suit::Wan, 1).unwrap())));
}

#[test]
fn parse_rejects_digits_without_suit() {
    assert!("123".parse::<TileCounts>().is_err());
    assert!("8z".parse::<TileCounts>().is_err());
}

#[test]
fn display_round_trips_and_groups_by_suit() {
    let counts: TileCounts = "7z55p321m".parse().unwrap();
    assert_eq!(counts.to_string(), "123m55p7z");
    assert_eq!(counts.to_string().parse::<TileCounts>(), Ok(counts));
}

#[test]
fn add_and_remove_track_length() {
    let mut counts = TileCounts::new();
    let tile = Tile::suited(Suit::Tiao, 3).unwrap();
    counts.add(tile);
    counts.add(tile);
    assert_eq!(counts.len(), 2);
    assert!(counts.remove(tile));
    assert!(counts.remove(tile));
    assert!(!counts.remove(tile));
    assert!(counts.is_empty());
}

#[test]
fn tiles_iterates_with_multiplicity_in_order() {
    let counts: TileCounts = "11z2m".parse().unwrap();
    let tiles: Vec<String> = counts.tiles().map(|t| t.to_string()).collect();
    assert_eq!(tiles, ["2m", "1z", "1z"]);
}
