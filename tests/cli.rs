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
fn sim_json_report_has_provenance() {
    let out = stdout(&mj(&["sim", "--hands", "50", "--json"], ""));
    let report: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(report["schema"], 1);
    assert_eq!(report["kind"], "simulation");
    assert_eq!(report["provenance"]["revision"].as_str().unwrap().len(), 40);
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
