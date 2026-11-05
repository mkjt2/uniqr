//! The parsed command line. `getopt.rs` fills it in; `cli.toml` documents it.

use std::ffi::OsString;

#[derive(Debug)]
pub struct Cli {
    pub input: Option<OsString>,
    pub output: Option<OsString>,
    pub count: bool,
    pub repeated: bool,
    pub all_repeated: Option<Septype>,
    pub skip_fields: Option<u32>,
    pub ignore_case: bool,
    pub skip_chars: Option<u32>,
    pub unique: bool,
}

/// The value of `-D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Septype {
    Nosep,
    Prepend,
    Separate,
}
