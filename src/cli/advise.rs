use std::sync::Arc;

use anyhow::{
    Context,
    bail,
};
use mjers_core::advisor::search::{
    SearchConfig,
    search,
};
use mjers_core::advisor::{
    Advice,
    AdviceConfig,
    advise,
};
use mjers_core::bot::encoding;
use mjers_core::bot::heuristic::HeuristicBot;
use mjers_core::bot::neural::NeuralNetwork;
use mjers_core::bot::policy::decide;
use mjers_core::bot::spec::BotSpec;
use mjers_core::game::Game;
use mjers_core::game::observation::Observation;
use mjers_core::game::trace::Trace;
use mjers_core::game::wall::Wall;
use mjers_core::rules::seat::Seat;

use super::load;

#[derive(clap::Args)]
pub struct Args {
    /// Replay this trace up to the decision instead of playing bots from --seed.
    #[arg(long)]
    trace: Option<String>,
    /// Seed of a hand played by efficiency bots.
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long, default_value = "family")]
    rules: String,
    /// How many decisions to play before advising.
    #[arg(long, default_value_t = 0)]
    step: usize,
    /// Advise this seat (default: the first seat to act).
    #[arg(long)]
    seat: Option<usize>,
    /// Playouts per candidate action; 0 for the instant ranking only.
    #[arg(long, default_value_t = 200)]
    samples: u32,
    #[arg(long, default_value_t = 6)]
    candidates: usize,
    /// Also show a neural strategy's choices and predicted settlement, and a
    /// search strategy's playouts of them (text output).
    #[arg(long)]
    network: Option<String>,
    #[arg(long)]
    json: bool,
}

pub fn run(args: Args) -> anyhow::Result<()> {
    let game = position(&args)?;
    let seat = match args.seat {
        Some(i) => Seat::new(i).context("seat must be 0-3")?,
        None => game.to_act().first().context("the hand is over at that step")?,
    };
    if !game.to_act().contains(seat) {
        bail!("{seat} has no decision at step {}", args.step);
    }
    let obs = game.observe(seat);
    let config = AdviceConfig {
        samples: args.samples,
        candidates: args.candidates,
        seed: args.seed,
        ..AdviceConfig::default()
    };
    let advice = advise(&obs, &config);
    if args.json {
        say!("{}", serde_json::to_string_pretty(&advice)?)?;
        return Ok(());
    }
    say!("{seat} holds {}; {} live draws left", obs.hand(), obs.live_wall())?;
    print_advice(&advice, usize::MAX)?;
    if let Some(name) = &args.network {
        print_network(&NetworkHint::load(name, "--network")?, &obs, args.seed)?;
    }
    Ok(())
}

fn position(args: &Args) -> anyhow::Result<Game> {
    if let Some(path) = &args.trace {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let trace: Trace = serde_json::from_str(&text).with_context(|| format!("parsing {path}"))?;
        let wall: [_; mjers_core::game::wall::WALL_SIZE] = trace.wall.clone().try_into().ok().context("trace wall must hold 144 tiles")?;
        let mut game = Game::from_wall(Arc::new(trace.rules.clone()), Wall::from_order(wall, trace.rules.reserved_tiles()), trace.dealer)?;
        for step in trace.steps.iter().take(args.step) {
            game.apply(step.seat, step.action)?;
        }
        return Ok(game);
    }
    let rules = Arc::new(load::ruleset(&args.rules)?);
    let mut game = Game::new(rules, args.seed, Seat::ALL[(args.seed % 4) as usize])?;
    let mut bot = HeuristicBot::efficiency();
    for _ in 0..args.step {
        let seat = game.to_act().first().context("the hand ended before that step")?;
        let action = decide(&game, seat, &mut bot);
        game.apply(seat, action)?;
    }
    Ok(game)
}

/// Prints playout estimates (if any) and the top `limit` ranked actions with reasons.
pub fn print_advice(
    advice: &Advice,
    limit: usize,
) -> anyhow::Result<()> {
    if !advice.estimates.is_empty() {
        say!("{:<22} {:>15} {:>16} {:>6} {:>8}", "playouts", "ev ± se", "vs top-ranked", "win%", "dealin%")?;
        for e in &advice.estimates {
            say!(
                "{:<22} {:>15} {:>16} {:>6.1} {:>8.1}",
                e.action.to_string(),
                format!("{:+.2} ± {:.2}", e.ev, e.ev_stderr),
                format!("{:+.2} ± {:.2}", e.difference, e.difference_stderr),
                100.0 * e.win_rate,
                100.0 * e.deal_in_rate
            )?;
        }
        say!()?;
    }
    say!("recommended: {}", advice.best())?;
    for a in advice.ranked.iter().take(limit) {
        say!("  {:<20} {}", a.action.to_string(), a.reasons.join("; "))?;
    }
    Ok(())
}

/// A network to show beside the advisor; a search strategy also plays out its candidates.
pub struct NetworkHint {
    network: NeuralNetwork,
    search: Option<SearchConfig>,
}

impl NetworkHint {
    pub fn load(
        name: &str,
        option: &str,
    ) -> anyhow::Result<NetworkHint> {
        match load::strategy(name)? {
            BotSpec::Neural { network, .. } => Ok(NetworkHint { network, search: None }),
            BotSpec::Search { network, search, .. } => Ok(NetworkHint { network, search: Some(*search) }),
            _ => bail!("{option} needs a neural or search strategy file"),
        }
    }
}

/// A network's three most likely legal actions and predicted settlement, then
/// the playouts and choice of a search, where it searched.
pub fn print_network(
    hint: &NetworkHint,
    observation: &Observation<'_>,
    seed: u64,
) -> std::io::Result<()> {
    let evaluation = hint.network.net().evaluate(&encoding::encode(observation));
    let top: Vec<String> = evaluation.choices(observation).iter().take(3).map(|(action, p)| format!("{action} {:.0}%", 100.0 * p)).collect();
    say!("network: {} (expects {:+.1} units)", top.join(", "), evaluation.settlement)?;
    let Some(config) = &hint.search else {
        return Ok(());
    };
    let result = search(&hint.network, observation, config, seed);
    for e in result.estimates.iter().skip(1) {
        say!("  playouts: {} {:+.2} ± {:.2} vs the network's choice", e.action, e.difference, e.difference_stderr)?;
    }
    match result.best(config.margin) {
        Some(best) if !result.estimates.is_empty() => say!("network search: {best}"),
        _ => Ok(()),
    }
}
