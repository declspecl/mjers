//! Records which source revision and toolchain built the binary, so reports
//! can say where they came from.

use std::process::Command;

const SOURCES: [&str; 13] = [
    "src",
    "build.rs",
    "Cargo.toml",
    "Cargo.lock",
    "mjers",
    "mjers-tile",
    "mjers-hand",
    "mjers-rules",
    "mjers-game",
    "mjers-bot",
    "mjers-advisor",
    "mjers-sim",
    "rulesets",
];

fn git(arguments: &[&str]) -> Option<String> {
    let output = Command::new("git").args(arguments).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    for path in SOURCES {
        println!("cargo:rerun-if-changed={path}");
    }
    for name in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }

    let revision = git(&["rev-parse", "--verify", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let mut status = vec!["status", "--porcelain", "--untracked-files=all", "--"];
    status.extend(SOURCES);
    let dirty = git(&status).map(|s| (!s.is_empty()).to_string()).unwrap_or_else(|| "unknown".into());
    let compiler = Command::new(std::env::var_os("RUSTC").expect("Cargo supplies RUSTC"))
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=MJERS_REVISION={revision}");
    println!("cargo:rustc-env=MJERS_DIRTY={dirty}");
    println!("cargo:rustc-env=MJERS_COMPILER={compiler}");
    for variable in ["TARGET", "PROFILE", "OPT_LEVEL"] {
        println!("cargo:rustc-env=MJERS_{variable}={}", std::env::var(variable).expect("Cargo supplies build settings"));
    }
}
