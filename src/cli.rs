//! Clap definitions: structure only.
//!
//! Help text, about strings and examples come from `cli.toml` and are applied
//! in [`command`], so do not add doc comments to these fields: clap would use
//! them as help text. `tests/conformance.rs` checks these definitions against
//! `cli.toml`.

use crate::generated::{self, FlagNames};
use clap::{ArgAction, Args, CommandFactory, Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(version, propagate_version = true, disable_help_subcommand = true)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
}

#[derive(Debug, Args)]
pub struct GlobalArgs {}

/// Effective global options. A field whose flag was removed with its kit
/// feature keeps its default.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub verbose: u8,
    pub quiet: bool,
    pub color: Option<ColorWhen>,
}

impl GlobalArgs {
    #[allow(clippy::needless_update)] // the update supplies defaults for removed flags
    pub fn settings(&self) -> Settings {
        Settings {
            ..Settings::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorWhen {
    Auto,
    Always,
    Never,
}

/// The clap command with all text from `cli.toml` applied.
pub fn command() -> clap::Command {
    let mut cmd = Cli::command()
        .name(generated::NAME)
        .bin_name(generated::NAME)
        .about(generated::ABOUT)
        .after_long_help(generated::ROOT_AFTER_LONG_HELP);
    if let Some(long) = generated::LONG_ABOUT {
        cmd = cmd.long_about(long);
    }
    let cmd = set_help(cmd, generated::ROOT_ARG_HELP);
    let cmd = apply_subcommands(cmd, "");
    apply_help_and_version_flags(cmd, generated::HELP_FLAGS, generated::VERSION_FLAGS)
}

/// Replace clap's `-h/--help` and `-V/--version` when `cli.toml` declares others
/// (e.g. `help_flags = ["--help"]` frees `-h` for the CLI's own use). `None`
/// keeps clap's flag.
fn apply_help_and_version_flags(
    mut cmd: clap::Command,
    help: Option<FlagNames>,
    version: Option<FlagNames>,
) -> clap::Command {
    if let Some(names) = help {
        cmd = disable_help_everywhere(cmd);
        cmd = add_flag(cmd, "help", names, ArgAction::Help, "Print help");
    }
    if let Some(names) = version {
        cmd = cmd.disable_version_flag(true);
        cmd = add_flag(cmd, "version", names, ArgAction::Version, "Print version");
    }
    cmd
}

fn disable_help_everywhere(cmd: clap::Command) -> clap::Command {
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    let mut cmd = cmd.disable_help_flag(true);
    for name in names {
        cmd = cmd.mut_subcommand(name, disable_help_everywhere);
    }
    cmd
}

fn add_flag(
    cmd: clap::Command,
    id: &'static str,
    (short, long): FlagNames,
    action: ArgAction,
    help: &'static str,
) -> clap::Command {
    if short.is_none() && long.is_none() {
        return cmd;
    }
    let mut arg = clap::Arg::new(id).action(action).help(help).global(true);
    if let Some(s) = short {
        arg = arg.short(s);
    }
    if let Some(l) = long {
        arg = arg.long(l);
    }
    cmd.arg(arg)
}

fn apply_subcommands(mut cmd: clap::Command, prefix: &str) -> clap::Command {
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in names {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix} {name}")
        };
        let Some(text) = generated::COMMANDS.iter().find(|c| c.path == path) else {
            // Missing entries are reported by the conformance test.
            continue;
        };
        cmd = cmd.mut_subcommand(name.as_str(), |mut sub| {
            sub = sub.about(text.about);
            if let Some(long) = text.long_about {
                sub = sub.long_about(long);
            }
            if let Some(after) = text.after_help {
                sub = sub.after_help(after);
            }
            apply_subcommands(set_help(sub, text.arg_help), &path)
        });
    }
    cmd
}

/// Set arguments' help text from (id, help) pairs. Arguments missing from the
/// code are skipped (the conformance test reports them) rather than panicking.
fn set_help(mut cmd: clap::Command, help: &[(&str, &'static str)]) -> clap::Command {
    for (id, text) in help {
        if cmd.get_arguments().any(|a| a.get_id() == id) {
            cmd = cmd.mut_arg(id, |a| a.help(*text));
        }
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(cmd: &clap::Command, args: &[&str]) -> Result<clap::ArgMatches, clap::Error> {
        cmd.clone()
            .try_get_matches_from(std::iter::once("x").chain(args.iter().copied()))
    }

    #[test]
    fn long_only_help_and_version_free_the_short_flags() {
        let cmd = apply_help_and_version_flags(
            Cli::command(),
            Some((None, Some("help"))),
            Some((None, Some("version"))),
        );
        cmd.clone().debug_assert();
        use clap::error::ErrorKind;
        let kind = |args: &[&str]| parse(&cmd, args).err().map(|e| e.kind());
        assert_eq!(kind(&["--help"]), Some(ErrorKind::DisplayHelp));
        assert_eq!(kind(&["--version"]), Some(ErrorKind::DisplayVersion));
        // -h/-V are free: unknown here, or the CLI's own flags.
        assert_ne!(kind(&["-h"]), Some(ErrorKind::DisplayHelp));
        assert_ne!(kind(&["-V"]), Some(ErrorKind::DisplayVersion));
    }

    #[test]
    fn help_flags_reach_subcommands_and_version_can_be_removed() {
        let cmd = apply_help_and_version_flags(
            Cli::command(),
            Some((Some('?'), Some("help"))),
            Some((None, None)),
        );
        cmd.clone().debug_assert();
        assert_eq!(
            parse(&cmd, &["-?"]).unwrap_err().kind(),
            clap::error::ErrorKind::DisplayHelp
        );
        assert_eq!(
            parse(&cmd, &["--version"]).unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
        let sub = cmd
            .get_subcommands()
            .next()
            .map(|s| s.get_name().to_string());
        if let Some(sub) = sub {
            assert_eq!(
                parse(&cmd, &[sub.as_str(), "-?"]).unwrap_err().kind(),
                clap::error::ErrorKind::DisplayHelp
            );
        }
    }

    #[test]
    fn missing_arguments_are_skipped() {
        let cmd = set_help(Cli::command(), &[("definitely_not_an_arg", "x")]);
        assert!(
            !cmd.get_arguments()
                .any(|a| a.get_id() == "definitely_not_an_arg")
        );
    }
}
