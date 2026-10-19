use mjers_sim::report::Provenance;

/// Build metadata captured by `build.rs`.
pub fn current() -> Provenance {
    Provenance {
        revision: env!("MJERS_REVISION").to_string(),
        dirty: env!("MJERS_DIRTY").parse().ok(),
        compiler: env!("MJERS_COMPILER").to_string(),
        target: env!("MJERS_TARGET").to_string(),
        profile: env!("MJERS_PROFILE").to_string(),
        opt_level: env!("MJERS_OPT_LEVEL").to_string(),
    }
}
