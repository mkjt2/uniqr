//! Shared helpers for integration tests.
#![allow(dead_code)]

use std::path::Path;
use std::sync::OnceLock;

/// The `cli.toml` model. The binary never parses the spec; the tests and
/// `examples/generate.rs` do.
pub mod spec;

/// The clap command rendered from the spec.
pub mod clap_model;

/// `cli.toml`, parsed once.
pub fn spec() -> &'static spec::Spec {
    static SPEC: OnceLock<spec::Spec> = OnceLock::new();
    SPEC.get_or_init(|| spec::Spec::parse(include_str!("../../cli.toml")).expect("cli.toml parses"))
}

/// `<NAME>_` prefix used by this CLI's environment variables.
pub fn env_prefix() -> String {
    format!(
        "{}_",
        env!("CARGO_PKG_NAME").to_uppercase().replace('-', "_")
    )
}

/// The binary under test with a hermetic environment: config and home point
/// at `home`, and variables that change output are removed. (The environment
/// is not cleared wholesale because coverage tooling relies on it.)
pub fn cmd(home: &Path) -> assert_cmd::Command {
    let mut c = assert_cmd::cargo::cargo_bin_cmd!();
    for (key, _) in std::env::vars() {
        if key.starts_with(&env_prefix()) {
            c.env_remove(key);
        }
    }
    c.env_remove("CI")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"));
    c
}
