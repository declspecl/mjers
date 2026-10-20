use super::*;

#[test]
fn every_preset_resolves_by_name() {
    for name in Ruleset::PRESETS {
        assert_eq!(Ruleset::preset(name).map(|r| r.name), Some(name.to_string()));
    }
    assert_eq!(Ruleset::preset("unknown"), None);
}

#[test]
fn family_matches_rules_md() {
    let rules = Ruleset::family();
    assert_eq!(rules.reserved_tiles(), 16);
    assert!(!rules.allow_rob_kong);
    let p = rules.payouts;
    assert_eq!((p.discard_win, p.discard_win_discarder, p.self_draw_win), (1, 2, 3));
    assert_eq!((p.per_bonus, p.exposed_kong, p.concealed_kong), (1, 1, 2));
    assert!(p.kongs_paid_without_winner);
}

#[test]
fn discarder_pays_keeps_the_discard_win_total() {
    let family = Ruleset::family().payouts;
    let variant = Ruleset::discarder_pays().payouts;
    assert_eq!(variant.discard_win * 2 + variant.discard_win_discarder, family.discard_win * 2 + family.discard_win_discarder);
}

#[test]
fn json_round_trip() {
    for name in Ruleset::PRESETS {
        let rules = Ruleset::preset(name).unwrap();
        let json = serde_json::to_string_pretty(&rules).unwrap();
        assert_eq!(serde_json::from_str::<Ruleset>(&json).unwrap(), rules);
    }
}

#[test]
fn json_rejects_unknown_fields() {
    let mut value = serde_json::to_value(Ruleset::family()).unwrap();
    value["typo"] = true.into();
    assert!(serde_json::from_value::<Ruleset>(value).is_err());
}

#[test]
fn checked_in_files_match_presets() {
    for name in Ruleset::PRESETS {
        let path = format!("{}/../rulesets/{name}.json", env!("CARGO_MANIFEST_DIR"));
        let file = std::fs::read_to_string(&path).unwrap();
        assert_eq!(serde_json::from_str::<Ruleset>(&file).unwrap(), Ruleset::preset(name).unwrap(), "{path}");
    }
}

#[test]
fn validation_rejects_flower_walls_that_starve_the_deal() {
    assert_eq!(Ruleset::family().validate(), Ok(()));
    let huge = Ruleset {
        flower_wall_stacks: 46,
        ..Ruleset::family()
    };
    assert_eq!(huge.validate(), Err(RulesError::FlowerWallTooLarge { stacks: 46 }));
    assert_eq!(
        Ruleset {
            flower_wall_stacks: 45,
            ..Ruleset::family()
        }
        .validate(),
        Ok(())
    );
}
