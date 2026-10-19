use super::*;

fn counts(notation: &str) -> TileCounts {
    notation.parse().unwrap()
}

fn tiles(ukeire: &Ukeire) -> String {
    ukeire.improvements.iter().map(|i| i.tile).collect::<TileCounts>().to_string()
}

#[test]
fn ready_hand_accepts_its_waits() {
    let result = ukeire(&counts("12345m456p789s11z"), 0, &TileCounts::new());
    assert_eq!(result.shanten, 0);
    assert_eq!(tiles(&result), "36m");
    // one 3m is already in hand
    assert_eq!(result.total(), 7);
}

#[test]
fn visible_tiles_reduce_live_counts() {
    let result = ukeire(&counts("12345m456p789s11z"), 0, &counts("6666m3m"));
    assert_eq!(tiles(&result), "3m");
    assert_eq!(result.total(), 2);
}

#[test]
fn improvements_lower_shanten() {
    let result = ukeire(&counts("123m456p78s1122z9m"), 0, &TileCounts::new());
    assert_eq!(result.shanten, 1);
    // 6s/9s complete 78s; 1z/2z complete a pong. A 9m pair adds nothing:
    // two sets plus two partials already fill the four blocks.
    assert_eq!(tiles(&result), "69s12z");
}

#[test]
fn best_discard_keeps_the_ready_shape() {
    let options = discard_options(&counts("12345m456p789s11z9m"), 0, &TileCounts::new());
    assert_eq!(options.len(), 13);
    let best = &options[0];
    assert_eq!(best.tile.to_string(), "9m");
    assert_eq!(best.ukeire.shanten, 0);
    assert_eq!(best.ukeire.total(), 7);
    assert!(options.windows(2).all(|w| w[0].ukeire.shanten <= w[1].ukeire.shanten));
}
