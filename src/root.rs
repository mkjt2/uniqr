//! The root command: open the operands, then run the engine.
//! Its operands and flags are fields on `Cli` in `src/cli.rs`.

use crate::cli::{Cli, Septype};
use crate::engine::{self, AllRepeated, Bytes, Failure, Mode};
use crate::error::{AppError, errno_message};
use crate::locale::{self, Charset, KeySpec, Lazy, SingleByteKeys, Utf8Keys, WideKeys};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, Read, Write};

pub fn run(stdout: &mut dyn Write, stdin: &mut dyn BufRead, cli: &Cli) -> Result<(), AppError> {
    let mode = Mode::new(
        cli.count,
        cli.repeated,
        cli.unique,
        cli.all_repeated.map(|s| match s {
            Septype::Nosep => AllRepeated::None,
            Septype::Prepend => AllRepeated::Prepend,
            Septype::Separate => AllRepeated::Separate,
        }),
    );

    // Same order as the original: the input is opened before the output is
    // created (or truncated).
    let in_name = match cli.input.as_deref() {
        Some(p) if p != "-" => p,
        _ => OsStr::new("stdin"),
    };
    let mut file_in;
    let input: &mut dyn Read = match cli.input.as_deref() {
        Some(p) if p != "-" => {
            file_in = File::open(p).map_err(|e| path_error(p, &e))?;
            &mut file_in
        }
        _ => stdin,
    };
    let out_name = cli.output.as_deref().unwrap_or(OsStr::new("stdout"));
    let mut file_out;
    let out: &mut dyn Write = match cli.output.as_deref() {
        Some(p) => {
            file_out = File::create(p).map_err(|e| path_error(p, &e))?;
            &mut file_out
        }
        None => stdout,
    };

    let spec = KeySpec {
        fields: u64::from(cli.skip_fields.unwrap_or(0)),
        chars: u64::from(cli.skip_chars.unwrap_or(0)),
        fold: cli.ignore_case,
    };
    let env = |k: &str| std::env::var(k).ok();
    let result = if let Some(lazy) = Lazy::new(spec, &env) {
        // setlocale(LC_ALL, "") happens on the first non-ASCII line, if any.
        engine::run(&lazy, mode, input, out)
    } else {
        // setlocale(LC_ALL, ""), as the original does.
        let loc = locale::detect();
        match loc.charset {
            _ if spec.is_plain() && !loc.collates => engine::run(&Bytes, mode, input, out),
            Charset::Utf8(a) => {
                let keys = Utf8Keys::new(spec, a, loc.collates, loc.ascii_distinct);
                engine::run(&keys, mode, input, out)
            }
            Charset::Single(t) => engine::run(
                &SingleByteKeys::new(spec, t, loc.collates),
                mode,
                input,
                out,
            ),
            Charset::Wide => engine::run(&WideKeys::new(spec, loc.collates), mode, input, out),
        }
    };
    result.map_err(|f| match f {
        Failure::Read(e) => path_error(in_name, &e),
        Failure::Write(e) => path_error(out_name, &e),
    })
}

/// `<path>: <strerror>`, as err(3) prints it.
fn path_error(path: &OsStr, e: &std::io::Error) -> AppError {
    AppError::Other(format!("{}: {}", path.to_string_lossy(), errno_message(e)))
}
