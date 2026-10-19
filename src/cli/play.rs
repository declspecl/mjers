use std::io::{
    BufRead,
    Write,
};
use std::sync::Arc;

use anyhow::{
    Context,
    bail,
};
use mjers::advisor::{
    AdviceConfig,
    advise,
};
use mjers::bot::{
    Policy,
    decide,
};
use mjers::game::rng::derive_seed;
use mjers::game::{
    Action,
    Event,
    Game,
    Seat,
};
use mjers::rules::WinSource;

use super::advise::print_advice;
use super::load;

#[derive(clap::Args)]
pub struct Args {
    /// Wall seed; random if omitted.
    #[arg(long)]
    seed: Option<u64>,
    /// Your seat, 0-3.
    #[arg(long, default_value_t = 0)]
    seat: usize,
    /// Ruleset preset or JSON file.
    #[arg(long, default_value = "family")]
    rules: String,
    /// Opponent strategy preset or JSON file.
    #[arg(long, default_value = "efficiency")]
    opponents: String,
    /// Show the advisor's top choices and reasons at each decision.
    #[arg(long)]
    hint: bool,
    /// Playouts per candidate for hints; 0 for instant hints.
    #[arg(long, default_value_t = 0)]
    hint_samples: u32,
    /// Let a bot play your seat too and show every tile.
    #[arg(long)]
    watch: bool,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let rules = Arc::new(load::ruleset(&args.rules)?);
    let opponents = load::strategy(&args.opponents)?;
    let seed = args.seed.unwrap_or_else(clock_seed);
    let human = Seat::new(args.seat).context("seat must be 0-3")?;
    let mut game = Game::new(rules, seed, Seat::ALL[(seed % 4) as usize]);
    let mut policies: [Box<dyn Policy>; 4] = std::array::from_fn(|i| opponents.build(derive_seed(seed, i as u64)));
    let viewer = (!args.watch).then_some(human);
    println!("seed {seed}, dealer {}, you are {human}", game.dealer());

    let mut shown = 0;
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    while let Some(&seat) = game.to_act().first() {
        shown = print_events(&game, shown, viewer);
        let action = if Some(seat) == viewer {
            show_state(&game, seat);
            if args.hint {
                let config = AdviceConfig {
                    samples: args.hint_samples,
                    seed,
                    ..AdviceConfig::default()
                };
                print_advice(&advise(&game.observe(seat), &config), 3);
            }
            prompt(&game, seat, &mut lines)?
        } else {
            decide(&game, seat, &mut *policies[seat.index()])
        };
        game.apply(seat, action)?;
    }
    print_events(&game, shown, viewer);
    let settlement = game.settlement().expect("hand is over");
    println!("result: {:?}", settlement.total());
    Ok(())
}

fn clock_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

/// Prints events from `from` onward as `viewer` may see them; returns the new count.
pub fn print_events(
    game: &Game,
    from: usize,
    viewer: Option<Seat>,
) -> usize {
    let sees = |seat: Seat| viewer.is_none_or(|v| v == seat);
    for event in &game.events()[from..] {
        let line = match *event {
            Event::Deal { seat, tiles } if sees(seat) => format!("{seat} dealt {tiles}"),
            Event::Deal { .. } => continue,
            Event::Bonus { seat, bonus } => format!("{seat} sets aside {bonus}"),
            Event::Draw { seat, tile } if sees(seat) => format!("{seat} draws {tile}"),
            Event::Draw { .. } => continue,
            Event::Discard { seat, tile } => format!("{seat} discards {tile}"),
            Event::Claim { seat, from, meld } => format!("{seat} claims {}'s tile: {}", from, meld.tiles()),
            Event::Kong { seat, meld } => format!("{seat} declares kong {}", meld.tiles()),
            Event::Win {
                seat,
                source: WinSource::SelfDraw,
                tile,
            } => format!("{seat} wins by self-draw on {tile}"),
            Event::Win {
                seat,
                source: WinSource::Discard(from),
                tile,
            } => format!("{seat} wins on {from}'s {tile}"),
            Event::Exhausted => "the wall is exhausted: no winner".to_string(),
        };
        println!("  {line}");
    }
    game.events().len()
}

fn show_state(
    game: &Game,
    seat: Seat,
) {
    let obs = game.observe(seat);
    println!();
    for other in seat.others() {
        let melds: Vec<String> = obs.melds(other).map(|m| m.tiles().to_string()).collect();
        let discards: Vec<String> = obs.discards().iter().filter(|d| d.seat == other).map(|d| d.tile.to_string()).collect();
        println!("{other}: {} hidden, melds [{}], discards {}", obs.concealed_count(other), melds.join(" "), discards.join(" "));
    }
    println!("wall: {} live draws left", obs.live_wall());
    println!("you:  {}", obs.hand());
}

fn prompt(
    game: &Game,
    seat: Seat,
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
) -> anyhow::Result<Action> {
    let legal = game.legal_actions(seat);
    for (i, action) in legal.iter().enumerate() {
        print!("[{i}] {action}  ");
    }
    println!();
    loop {
        print!("> ");
        std::io::stdout().flush()?;
        let Some(line) = lines.next() else {
            bail!("input closed");
        };
        let line = line?;
        let input = line.trim();
        if let Some(action) = input.parse::<usize>().ok().and_then(|i| legal.get(i)) {
            return Ok(*action);
        }
        if let Some(action) = legal.iter().find(|a| matches!(a, Action::Discard(t) if t.to_string() == input) || a.to_string() == input) {
            return Ok(*action);
        }
        println!("enter a number from the list, or a tile to discard");
    }
}
