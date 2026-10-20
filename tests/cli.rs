#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]

use std::io::Write;
use std::process::{
    Command,
    Output,
    Stdio,
};

fn mj(
    args: &[&str],
    stdin: &str,
) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mj"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(output: &Output) -> String {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout.clone()).unwrap()
}

#[test]
fn analyze_ready_hand_lists_waits() {
    let out = stdout(&mj(&["analyze", "12345m456p789s11z"], ""));
    assert!(out.contains("shanten  0"), "{out}");
    assert!(out.contains("waits    3m×3 6m×4 (7 live)"), "{out}");
}

#[test]
fn analyze_full_hand_ranks_discards() {
    let out = stdout(&mj(&["analyze", "12345m456p789s11z9m"], ""));
    let first = out
        .lines()
        .find(|l| l.starts_with("discard"))
        .and_then(|_| out.lines().skip_while(|l| !l.starts_with("discard")).nth(1))
        .unwrap();
    assert!(first.starts_with("9m"), "{out}");
}

#[test]
fn analyze_rejects_wrong_sizes() {
    assert!(!mj(&["analyze", "123m"], "").status.success());
}

#[test]
fn watch_mode_plays_a_whole_hand() {
    let out = stdout(&mj(&["play", "--watch", "--seed", "7"], ""));
    assert!(out.contains("seed 7"), "{out}");
    assert!(out.contains("result: ["), "{out}");
}

#[test]
fn interactive_play_accepts_tile_input() {
    // Discard whatever is first each time, by number; stop when input runs out.
    let input = "0\n".repeat(200);
    let out = mj(&["play", "--seed", "3", "--hint"], &input);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("recommended: "), "{text}");
    assert!(text.contains("you:  "), "{text}");
}

#[test]
fn unknown_rules_fail_clearly() {
    let out = mj(&["play", "--watch", "--rules", "nope"], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a preset"));
}

#[test]
fn sim_prints_metrics_by_strategy_and_position() {
    let out = stdout(&mj(&["sim", "--hands", "200", "--seats", "efficiency,defensive,efficiency,random"], ""));
    for label in ["defensive", "efficiency", "random", "dealer+3", "ev ± se"] {
        assert!(out.contains(label), "{label}: {out}");
    }
}

#[test]
fn sim_json_report_has_experiment_inputs() {
    let out = stdout(&mj(&["sim", "--hands", "50", "--json"], ""));
    let report: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(report["schema"], 2);
    assert_eq!(report["kind"], "simulation");
    assert_eq!(report["rules"]["name"], "family");
    assert_eq!(report["seeds"]["start"], 0);
    assert_eq!(report["seeds"]["count"], 50);
    assert_eq!(report["strategies"].as_array().unwrap().len(), 4);
}

#[test]
fn sim_writes_feature_rows() {
    let dir = std::env::temp_dir().join(format!("mj-features-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rows.jsonl");
    stdout(&mj(&["sim", "--hands", "25", "--features", path.to_str().unwrap()], ""));
    assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 100);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn compare_reports_a_difference() {
    let out = stdout(&mj(&["compare", "efficiency", "random", "--hands", "100"], ""));
    assert!(out.contains("difference +"), "{out}");
    assert!(out.contains("z = "), "{out}");
}

#[test]
fn traces_replay_through_the_cli() {
    let trace = stdout(&mj(&["trace", "--seed", "4"], ""));
    let dir = std::env::temp_dir().join(format!("mj-trace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("trace.json");
    std::fs::write(&path, trace).unwrap();
    let out = stdout(&mj(&["replay", path.to_str().unwrap()], ""));
    assert!(out.contains("replayed"), "{out}");
    assert!(out.contains("seat 3: "), "{out}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn strategy_files_load() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/strategies/no-chow.json");
    let out = stdout(&mj(&["compare", path, "efficiency", "--hands", "20"], ""));
    assert!(out.contains("no-chow vs efficiency"), "{out}");
}

#[test]
fn tune_writes_a_loadable_strategy() {
    let dir = std::env::temp_dir().join(format!("mj-tune-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tuned.json");
    let out = stdout(&mj(&["tune", "--hands", "20", "--rounds", "1", "--output", path.to_str().unwrap()], ""));
    assert!(out.contains("held-out seeds 20..40"), "{out}");
    let spec: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(spec["name"], "tuned");
    stdout(&mj(&["compare", path.to_str().unwrap(), "efficiency", "--hands", "10"], ""));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn advise_explains_a_bot_hand_decision() {
    let out = stdout(&mj(&["advise", "--seed", "2", "--step", "9", "--samples", "20", "--candidates", "3"], ""));
    assert!(out.contains("playouts"), "{out}");
    assert!(out.contains("recommended: "), "{out}");
    assert!(out.contains("live"), "{out}");
}

#[test]
fn advise_reads_traces() {
    let trace = stdout(&mj(&["trace", "--seed", "8"], ""));
    let dir = std::env::temp_dir().join(format!("mj-advise-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("trace.json");
    std::fs::write(&path, trace).unwrap();
    let out = stdout(&mj(&["advise", "--trace", path.to_str().unwrap(), "--step", "4", "--samples", "0", "--json"], ""));
    let advice: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(!advice["ranked"].as_array().unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn study_prints_every_section() {
    let out = stdout(&mj(&["study", "--hands", "100"], ""));
    for section in ["winning hands by set split", "wait shape when first ready", "tile fates", "dragon", "realized%"] {
        assert!(out.contains(section), "{section}: {out}");
    }
}

/// A tiny `suit-resnet-v2` safetensors checkpoint trained (nominally) on seeds 0 and 1.
fn write_checkpoint(path: &std::path::Path) {
    let (width, planes, context) = (2usize, 50usize, 22usize);
    let mut layers: Vec<(String, Vec<usize>)> = Vec::new();
    let mut layer = |name: &str, weight: Vec<usize>| {
        let outputs = weight[0];
        layers.push((format!("{name}.weight"), weight));
        layers.push((format!("{name}.bias"), vec![outputs]));
    };
    layer("stem", vec![width, planes + context, 1]);
    layer("blocks.0.suit_first", vec![width, width, 1, 3]);
    layer("blocks.0.suit_second", vec![width, width, 1, 3]);
    layer("blocks.0.honor_first", vec![width, width, 1]);
    layer("blocks.0.honor_second", vec![width, width, 1]);
    layer("blocks.0.pooled", vec![width, 2 * width]);
    layer("tile_actions", vec![3, width, 1]);
    layer("summary", vec![width, 3 * width + context]);
    layer("other_actions", vec![8, width]);
    layer("value_layer", vec![1, width]);
    let rules = serde_json::to_string(&mjers_core::rules::Ruleset::family()).unwrap();
    let metadata = serde_json::json!({
        "architecture": "suit-resnet-v2",
        "hidden_width": width,
        "dataset": {"encoding_version": 1, "rules_json": rules, "teacher_json": "{}"},
        "training_seeds": [0],
        "validation_seeds": [1],
        "reinforcement": [{"field_json": "{}", "start_seed": 500, "hands": 10}],
    });
    let mut header = serde_json::Map::new();
    header.insert("__metadata__".into(), serde_json::json!({"mjers": metadata.to_string()}));
    let mut data = Vec::new();
    for (i, (name, shape)) in layers.iter().enumerate() {
        let begin = data.len();
        for j in 0..shape.iter().product::<usize>() {
            data.extend_from_slice(&(((i * 7 + j * 3) % 11) as f32 / 20.0 - 0.25).to_le_bytes());
        }
        header.insert(name.clone(), serde_json::json!({"dtype": "F32", "shape": shape, "data_offsets": [begin, data.len()]}));
    }
    let header = serde_json::to_vec(&header).unwrap();
    let mut file = (header.len() as u64).to_le_bytes().to_vec();
    file.extend(header);
    file.extend(data);
    std::fs::write(path, file).unwrap();
}

#[test]
fn neural_strategies_play_only_on_unseen_seeds_and_rules() {
    let dir = std::env::temp_dir().join(format!("mj-neural-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let checkpoint = dir.join("policy.safetensors");
    write_checkpoint(&checkpoint);
    let strategy = dir.join("net.json");
    std::fs::write(&strategy, serde_json::json!({"kind": "neural", "name": "net", "network": checkpoint}).to_string()).unwrap();
    let path = strategy.to_str().unwrap();
    let out = stdout(&mj(&["compare", path, "efficiency", "--hands", "4", "--seed", "100"], ""));
    assert!(out.contains("net vs efficiency"), "{out}");
    let json = stdout(&mj(&["compare", path, "efficiency", "--hands", "2", "--seed", "100", "--json"], ""));
    assert!(json.contains(checkpoint.to_str().unwrap()), "reports name the checkpoint: {json}");
    let overlap = mj(&["compare", path, "efficiency", "--hands", "4", "--seed", "0"], "");
    assert!(!overlap.status.success() && String::from_utf8_lossy(&overlap.stderr).contains("overlap"));
    let reinforced = mj(&["compare", path, "efficiency", "--hands", "4", "--seed", "508"], "");
    assert!(!reinforced.status.success() && String::from_utf8_lossy(&reinforced.stderr).contains("overlap"), "PPO seeds are checked");
    let rules = mj(&["compare", path, "efficiency", "--hands", "4", "--seed", "100", "--rules", "discarder-pays"], "");
    assert!(!rules.status.success() && String::from_utf8_lossy(&rules.stderr).contains("different rules"));
    let hybrid = dir.join("hybrid.json");
    std::fs::write(&hybrid, serde_json::json!({"kind": "hybrid", "name": "net-turns", "turns": path, "claims": "efficiency"}).to_string()).unwrap();
    let hybrid = hybrid.to_str().unwrap();
    let out = stdout(&mj(&["compare", hybrid, "efficiency", "--hands", "4", "--seed", "100"], ""));
    assert!(out.contains("net-turns vs efficiency"), "{out}");
    let nested = mj(&["compare", hybrid, "efficiency", "--hands", "4", "--seed", "0"], "");
    assert!(!nested.status.success() && String::from_utf8_lossy(&nested.stderr).contains("overlap"), "hybrid parts are checked");
    let hinted = mj(&["play", "--seed", "7", "--hint-network", path], "");
    assert!(String::from_utf8_lossy(&hinted.stdout).contains("network: "), "play shows network hints");
    let advised = stdout(&mj(&["advise", "--seed", "100", "--step", "3", "--samples", "0", "--network", path], ""));
    assert!(advised.contains("network: "), "{advised}");
    let study = mj(&["study", "--strategy", path, "--hands", "2", "--seed", "0"], "");
    assert!(!study.status.success() && String::from_utf8_lossy(&study.stderr).contains("overlap"), "studies are checked too");
    let tune = mj(&["tune", "--field", path, "--hands", "1", "--rounds", "1", "--seed", "0"], "");
    assert!(!tune.status.success() && String::from_utf8_lossy(&tune.stderr).contains("overlap"), "tuning fields are checked");
    std::fs::remove_dir_all(dir).unwrap();
}
