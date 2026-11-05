//! Render documentation from `cli.toml`: `--help` for `src/help.txt`, the man page
//! and shell completions for release archives, and the interface as JSON.
//!
//! Usage: cargo run --example generate -- help|man|schema|complete-<shell>
//!
//! It's an example so that clap stays a dev-dependency, out of the shipped binary.

#[allow(dead_code)]
#[path = "../tests/common/spec.rs"]
mod spec;

#[path = "../tests/common/clap_model.rs"]
mod clap_model;

use std::io::Write;

fn main() {
    let what = std::env::args().nth(1).unwrap_or_default();
    let spec = spec::Spec::parse(include_str!("../cli.toml")).expect("cli.toml parses");
    let Some(out) = clap_model::render(&spec, &what) else {
        eprintln!(
            "usage: generate {}|complete-<shell>",
            clap_model::TARGETS.join("|")
        );
        std::process::exit(2);
    };
    std::io::stdout().write_all(&out).expect("write to stdout");
}
