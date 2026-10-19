use anyhow::bail;
use mjers_hand::shanten;
use mjers_hand::ukeire::Ukeire;
use mjers_tile::counts::TileCounts;

#[derive(clap::Args)]
pub struct Args {
    /// Concealed tiles, e.g. 123m456p78s1122z9m.
    hand: TileCounts,
    /// Melds already declared beside the hand.
    #[arg(long, default_value_t = 0)]
    melds: usize,
    /// Tiles you can see elsewhere (discards, others' melds).
    #[arg(long, default_value_t = TileCounts::new())]
    visible: TileCounts,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let size = args.hand.len() + 3 * args.melds;
    let s = shanten::shanten(&args.hand, args.melds);
    println!("hand     {} (+{} melds)", args.hand, args.melds);
    match size {
        13 => {
            println!("shanten  {s}");
            print_ukeire(&mjers_hand::ukeire::ukeire(&args.hand, args.melds, &args.visible));
        },
        14 if s == -1 => println!("complete hand"),
        14 => {
            println!("shanten  {s}");
            println!();
            println!("discard  shanten  live  improving tiles");
            for option in mjers_hand::ukeire::discard_options(&args.hand, args.melds, &args.visible) {
                println!(
                    "{:<8} {:>7}  {:>4}  {}",
                    option.tile.to_string(),
                    option.ukeire.shanten,
                    option.ukeire.total(),
                    improving(&option.ukeire)
                );
            }
        },
        _ => bail!("a hand holds 13 or 14 tiles counting 3 per meld, not {size}"),
    }
    Ok(())
}

fn print_ukeire(ukeire: &Ukeire) {
    let label = if ukeire.shanten == 0 { "waits" } else { "improving" };
    println!("{label:<9}{} ({} live)", improving(ukeire), ukeire.total());
}

fn improving(ukeire: &Ukeire) -> String {
    ukeire.improvements().map(|i| format!("{}×{}", i.tile, i.live)).collect::<Vec<_>>().join(" ")
}
