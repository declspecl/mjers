use std::collections::BTreeMap;

use mjers_sim::report::Report;
use mjers_sim::runner::{
    Seeds,
    Table,
};
use mjers_sim::study::{
    CLASSES,
    Study,
    Tally,
    study,
};

use super::{
    load,
    provenance,
};

#[derive(clap::Args)]
pub struct Args {
    #[arg(long, default_value = "family")]
    rules: String,
    /// Strategy for every seat (preset or JSON file).
    #[arg(long, default_value = "efficiency")]
    strategy: String,
    #[arg(long, default_value_t = 100_000)]
    hands: u64,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let rules = load::ruleset(&args.rules)?;
    let spec = load::strategy(&args.strategy)?;
    let table: Table = std::array::from_fn(|_| spec.clone());
    let seeds = Seeds { start: args.seed, count: args.hands };
    let s = study(&rules, &table, seeds);
    if args.json {
        let report = Report::new("study", provenance::current(), rules, seeds, vec![spec], s);
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    println!("{} hands, rules {}, every seat {}", s.hands, rules.name, spec.name());
    print_splits(&s);
    print_composition(&s);
    print_tallies("exposed melds at the end (all seats)", "exposed", &s.exposed, false);
    print_tallies("wait shape when first ready", "shape", &s.ready_shapes, true);
    print_tallies("different tiles waited on when first ready", "tiles", &s.ready_waits, true);
    let live: BTreeMap<String, Tally> = s.ready_live.iter().map(|(k, v)| (if *k == 8 { "8+".to_string() } else { k.to_string() }, v.clone())).collect();
    print_tallies("live winning tiles when first ready", "live", &live, true);
    print_tallies("single-tile eye waits (tanki) by tile", "tile", &s.tanki_tiles, true);
    print_fates(&s);
    Ok(())
}

fn print_splits(s: &Study) {
    let wins: u64 = s.winning_splits.values().map(|t| t.count).sum();
    println!();
    println!("winning hands by set split ({wins} wins; c = chows, p = pongs, k = kongs)");
    println!("{:<12} {:>8} {:>10}", "split", "share", "units won");
    let mut rows: Vec<_> = s.winning_splits.iter().collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1.count));
    for (key, t) in rows {
        println!("{key:<12} {:>7.1}% {:>10.2}", 100.0 * t.count as f64 / wins.max(1) as f64, t.mean_units());
    }
    println!();
    println!("eye of winning hands");
    for class in CLASSES {
        if let Some(t) = s.winning_eyes.get(class) {
            print!("{class}: {:.1}%  ", 100.0 * t.count as f64 / wins.max(1) as f64);
        }
    }
    println!();
}

fn print_composition(s: &Study) {
    let c = &s.composition;
    let per = |n: u64| n as f64 / c.hands.max(1) as f64;
    let pct = |n: u64, of: u64| 100.0 * n as f64 / of.max(1) as f64;
    println!();
    println!("average winning hand (sets per hand)");
    println!("{:<8} {:>8} {:>10}", "", "exposed", "concealed");
    println!("{:<8} {:>8.2} {:>10.2}", "chows", per(c.exposed_chows), per(c.concealed_chows));
    println!("{:<8} {:>8.2} {:>10.2}", "pongs", per(c.exposed_pongs), per(c.concealed_pongs));
    println!("{:<8} {:>8.3} {:>10.3}", "kongs", per(c.exposed_kongs), per(c.concealed_kongs));
    let chows = c.terminal_chows + c.inner_chows;
    println!();
    println!("chows in winning hands");
    println!("  touching a terminal (123/789): {:.1}%, inner: {:.1}%", pct(c.terminal_chows, chows), pct(c.inner_chows, chows));
    println!(
        "  claimed chows completed: two-sided {:.1}%, gap {:.1}%, edge {:.1}%",
        pct(c.claimed_two_sided, c.exposed_chows),
        pct(c.claimed_gap, c.exposed_chows),
        pct(c.claimed_edge, c.exposed_chows)
    );
    println!("  hands with 2+ chows: {:.1}% of wins; of those, chows span", pct(c.multi_chow_hands, c.hands));
    println!(
        "    1 suit {:.1}%, 2 suits {:.1}%, 3 suits {:.1}%",
        pct(c.chow_suits[1], c.multi_chow_hands),
        pct(c.chow_suits[2], c.multi_chow_hands),
        pct(c.chow_suits[3], c.multi_chow_hands)
    );
    println!(
        "    4+ in a row {:.1}% (overlapping like 345+456: {:.1}%, back to back like 456+789: {:.1}%)",
        pct(c.run_of_four_plus, c.multi_chow_hands),
        pct(c.overlapping, c.multi_chow_hands),
        pct(c.back_to_back, c.multi_chow_hands)
    );
    println!(
        "    same chow twice {:.1}%, full 123456789 straight {:.1}%",
        pct(c.repeated, c.multi_chow_hands),
        pct(c.straight, c.multi_chow_hands)
    );
}

fn print_tallies(
    title: &str,
    label: &str,
    tallies: &BTreeMap<String, Tally>,
    show_live: bool,
) {
    println!();
    println!("{title}");
    let extra = if show_live {
        format!(" {:>8} {:>7} {:>8} {:>9}", "live", "self%", "turns", "own draws")
    } else {
        String::new()
    };
    println!("{label:<24} {:>9} {:>7} {:>8}{extra}", "seats", "win%", "ev");
    let mut rows: Vec<_> = tallies.iter().collect();
    rows.sort_by(|a, b| b.1.win_rate().total_cmp(&a.1.win_rate()));
    for (key, t) in rows {
        let live = if show_live {
            format!(
                " {:>8.1} {:>6.1}% {:>8.1} {:>9.1}",
                t.mean_live(),
                100.0 * t.self_draw_share(),
                t.mean_table_turns(),
                t.mean_own_draws()
            )
        } else {
            String::new()
        };
        println!("{key:<24} {:>9} {:>6.1}% {:>+8.2}{live}", t.count, 100.0 * t.win_rate(), t.mean_units());
    }
}

fn print_fates(s: &Study) {
    println!();
    println!("tile fates, % of copies that entered a hand (suits combined by rank)");
    println!(
        "{:<7} {:>9} {:>9} {:>9} {:>9} {:>8} {:>8} {:>8} {:>9} {:>9}",
        "tile", "entered/h", "discard%", "claimed%", "won-on%", "meld%", "in-win%", "stuck%", "isolated%", "realized%"
    );
    for (class, f) in CLASSES.iter().zip(s.tiles_by_class()) {
        let of = |n: u64| 100.0 * n as f64 / f.entered.max(1) as f64;
        println!(
            "{class:<7} {:>9.2} {:>9.1} {:>9.1} {:>9.1} {:>8.1} {:>8.1} {:>8.1} {:>9.1} {:>9.1}",
            f.entered as f64 / s.hands.max(1) as f64,
            of(f.discarded),
            of(f.discards_claimed),
            of(f.discards_won_on),
            of(f.melded),
            of(f.in_win),
            of(f.held_at_end),
            of(f.isolated_at_end),
            of(f.melded + f.in_win),
        );
    }
    println!("claimed% and won-on% are discards taken by others; realized% = melded + in a win.");
}
