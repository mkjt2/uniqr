//! The interface in `cli.toml` as a clap command. It only renders documentation:
//! `--help` (kept in `src/help.txt`), the man page and shell completions. The
//! shipped binary parses arguments with `src/getopt.rs` and never links clap.

use super::spec::{Flag, FlagKind, Spec};
use clap::{Arg, ArgAction, Command};

/// Everything `render` accepts, besides `complete-<shell>`.
pub const TARGETS: &[&str] = &["help", "man", "schema"];

fn flag(f: &Flag) -> Arg {
    let mut arg = Arg::new(f.id.clone())
        .long(f.long.clone())
        .help(f.help.clone());
    if let Some(short) = f.short {
        arg = arg.short(short);
    }
    if let Some(name) = &f.value_name {
        arg = arg.value_name(name.clone());
    }
    if !f.possible_values.is_empty() {
        arg = arg.value_parser(f.possible_values.clone());
    }
    if let Some(missing) = &f.default_missing {
        arg = arg
            .num_args(0..=1)
            .require_equals(true)
            .default_missing_value(missing.clone());
    }
    match f.kind {
        FlagKind::Switch => arg.action(ArgAction::SetTrue),
        FlagKind::Count => arg.action(ArgAction::Count),
        FlagKind::Value => arg.action(ArgAction::Set),
        FlagKind::Values => arg.action(ArgAction::Append),
    }
}

/// The clap command for `spec`, with `--help` and `--version` long-only.
pub fn command(spec: &Spec) -> Command {
    let cli = &spec.cli;
    let mut cmd = Command::new(cli.name.clone())
        .bin_name(cli.name.clone())
        .version(env!("CARGO_PKG_VERSION"))
        .about(cli.about.clone())
        .after_long_help(spec.root_after_help())
        .disable_help_flag(true)
        .disable_version_flag(true)
        .term_width(100);
    if let Some(long) = &cli.long_about {
        cmd = cmd.long_about(long.clone());
    }
    for a in &spec.args {
        let mut arg = Arg::new(a.id.clone()).help(a.help.clone());
        if let Some(name) = &a.value_name {
            arg = arg.value_name(name.clone());
        }
        cmd = cmd.arg(arg);
    }
    for f in &spec.flags {
        cmd = cmd.arg(flag(f));
    }
    cmd.arg(
        Arg::new("help")
            .long("help")
            .action(ArgAction::Help)
            .help("Print help"),
    )
    .arg(
        Arg::new("version")
            .long("version")
            .action(ArgAction::Version)
            .help("Print version"),
    )
}

/// Render `what`, or `None` if it is not a known target. Completions go through
/// memory because clap_complete panics on write errors.
pub fn render(spec: &Spec, what: &str) -> Option<Vec<u8>> {
    let mut cmd = command(spec);
    let mut out = Vec::new();
    match what {
        "help" => out = format!("{}", cmd.render_long_help()).into_bytes(),
        "man" => clap_mangen::Man::new(cmd)
            .render(&mut out)
            .expect("write to memory"),
        "schema" => {
            let doc = spec.schema_document(env!("CARGO_PKG_VERSION"));
            out = format!("{doc}\n").into_bytes();
        }
        _ => {
            let shell = what
                .strip_prefix("complete-")?
                .parse::<clap_complete::Shell>()
                .ok()?;
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut out);
        }
    }
    Some(out)
}
