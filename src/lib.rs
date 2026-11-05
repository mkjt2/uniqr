//! Library half of the CLI: everything except process setup lives here so
//! it can be tested in-process. `main.rs` only wires real stdio into [`run`].

pub mod cli;
pub mod engine;
pub mod error;
pub mod getopt;
pub mod locale;
pub mod root;

use error::AppError;
use std::ffi::OsString;
use std::io::{BufRead, ErrorKind, Write};

/// The CLI's name, used in messages until argv\[0\] is known.
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// `--help`: rendered from `cli.toml` by `just snapshots`, and checked against
/// it by `tests/conformance.rs`.
pub const HELP: &str = include_str!("help.txt");

/// `--version`.
pub const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"), "\n");

/// Process-level inputs, injectable for tests.
pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
    pub input: &'a mut dyn BufRead,
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

/// Parse `args` (including argv\[0\]), run the command, and return the exit code.
pub fn run(args: Vec<OsString>, io: Io) -> i32 {
    if let Some(argv0) = args.first() {
        error::set_progname(argv0);
    }
    let result = match getopt::parse(&args, (io.env)("POSIXLY_CORRECT").is_some()) {
        Ok(getopt::Parsed::Run(cli)) => root::run(io.out, io.input, &cli),
        Ok(getopt::Parsed::Help) => {
            let _ = io.out.write_all(HELP.as_bytes());
            Ok(())
        }
        Ok(getopt::Parsed::Version) => {
            let _ = io.out.write_all(VERSION.as_bytes());
            Ok(())
        }
        Err(getopt::ArgError::Usage(m)) => Err(AppError::BsdUsage(m)),
        Err(getopt::ArgError::Fatal(m)) => Err(AppError::Other(m)),
    };
    match result {
        Ok(()) => 0,
        Err(e) => report(&e, io.err),
    }
}

/// Write `err` to stderr and return its exit code. A closed stdout pipe
/// (`| head`) is not an error: it exits quietly with 0.
pub fn report(err: &AppError, out: &mut dyn Write) -> i32 {
    if matches!(err, AppError::Io(e) if e.kind() == ErrorKind::BrokenPipe) {
        return 0;
    }
    let text = error::render(err, error::progname());
    // Only newlines: a message can end in meaningful blanks ("invalid: ").
    let _ = writeln!(out, "{}", text.trim_end_matches('\n'));
    error::EXIT_CODE
}

/// Flush buffered stdout at exit, keeping `code` unless flushing fails for a
/// reason other than a closed pipe.
pub fn flush_stdout(out: &mut dyn Write, err: &mut dyn Write, code: i32) -> i32 {
    match out.flush() {
        Ok(()) => code,
        Err(e) if e.kind() == ErrorKind::BrokenPipe => code,
        Err(e) => report(&AppError::Io(e), err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Failing(ErrorKind);

    impl Write for Failing {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(self.0.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(self.0.into())
        }
    }

    #[test]
    fn broken_pipe_is_quiet_success() {
        let mut err = Vec::new();
        let e = AppError::Io(ErrorKind::BrokenPipe.into());
        assert_eq!(report(&e, &mut err), 0);
        assert!(err.is_empty());
        assert_eq!(
            flush_stdout(&mut Failing(ErrorKind::BrokenPipe), &mut err, 3),
            3
        );
        assert!(err.is_empty());
    }

    #[test]
    fn other_flush_failures_are_errors() {
        let mut err = Vec::new();
        let code = flush_stdout(&mut Failing(ErrorKind::Other), &mut err, 0);
        assert_eq!(code, error::EXIT_CODE);
        assert!(!err.is_empty());
        assert_eq!(flush_stdout(&mut Vec::new(), &mut err, 7), 7);
    }
}
