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
    assert!(text.contains("hint: "), "{text}");
    assert!(text.contains("you:  "), "{text}");
}

#[test]
fn unknown_rules_fail_clearly() {
    let out = mj(&["play", "--watch", "--rules", "nope"], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a preset"));
}
