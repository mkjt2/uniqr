//! What the binary needs from `cli.toml`: text, help/version flags, exit codes
//! and the schema. `build.rs` renders these from the spec at compile time, so
//! nothing parses `cli.toml` at runtime.

/// How errors are shown to humans (`[cli].error_style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorStyle {
    /// miette diagnostics: code, message and hint.
    Rich,
    /// `<name>: <message>`, plus a `hint:` line when there is one.
    Plain,
}

/// Declared help or version flags as (short, long).
pub type FlagNames = (Option<char>, Option<&'static str>);

/// Help text for one `[[command]]`.
#[derive(Debug)]
pub struct CommandText {
    /// Space-separated path, e.g. `config show`.
    pub path: &'static str,
    pub about: &'static str,
    pub long_about: Option<&'static str>,
    /// The examples section, shown after the help.
    pub after_help: Option<&'static str>,
    /// (arg id, help) for its flags and positionals.
    pub arg_help: &'static [(&'static str, &'static str)],
}

// NAME, ABOUT, LONG_ABOUT, ROOT_AFTER_LONG_HELP, ROOT_ARG_HELP, COMMANDS,
// HELP_FLAGS and VERSION_FLAGS (`None`: clap's own), KIT_JSON, ERROR_STYLE,
// ERROR_EXIT_CODE, USAGE_EXIT_CODE and SCHEMA_JSON.
include!(concat!(env!("OUT_DIR"), "/spec_gen.rs"));
