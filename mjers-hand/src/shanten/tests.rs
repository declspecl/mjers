use super::*;

fn of(
    notation: &str,
    melds: usize,
) -> i8 {
    shanten(&notation.parse().unwrap(), melds)
}

#[test]
fn complete_hands_are_minus_one() {
    assert_eq!(of("123m456p789s11122z", 0), -1);
    assert_eq!(of("11223344556677m", 0), -1);
    assert_eq!(of("55z", 4), -1);
}

#[test]
fn ready_hands_are_zero() {
    assert_eq!(of("123m456p789s1112z", 0), 0);
    assert_eq!(of("1112345678999m", 0), 0);
    assert_eq!(of("1z", 4), 0);
}

#[test]
fn counts_sets_partials_and_eye() {
    // two sets, eye, two partials
    assert_eq!(of("123m456p78s1122z9m", 0), 1);
    // three sets, four isolated honors
    assert_eq!(of("123456789m1234z", 0), 2);
    // partials across a suit boundary do not count
    assert_eq!(of("9m1p9p1s9s1234567z", 0), 8);
}

#[test]
fn special_hands_score_as_standard_form() {
    assert_eq!(of("19m19p19s1234567z", 0), 8);
    // seven pairs: an eye and four pair partials, but only four blocks count
    assert_eq!(of("1122m3344p5566s7z", 0), 3);
}

#[test]
fn partials_are_capped_by_open_slots() {
    // one meld declared: five partials compete for three open slots
    assert_eq!(of("12m45m78m12p45p", 1), 3);
}

#[test]
fn isolated_honors_need_pairs() {
    assert_eq!(of("1z2z3z4z", 3), 2);
}
