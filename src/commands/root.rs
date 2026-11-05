//! The root command: open the operands, then run the engine.
//! Its operands and flags are fields on `Cli` in `src/cli.rs`.

use crate::cli::{Cli, Septype};
use crate::ctx::Ctx;
use crate::engine::{self, AllRepeated, Bytes, Failure, Mode};
use crate::error::{AppError, errno_message};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};

pub fn run(ctx: &mut Ctx, cli: &Cli) -> Result<(), AppError> {
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
        _ => ctx.input,
    };
    let out_name = cli.output.as_deref().unwrap_or(OsStr::new("stdout"));
    let mut file_out;
    let out: &mut dyn Write = match cli.output.as_deref() {
        Some(p) => {
            file_out = File::create(p).map_err(|e| path_error(p, &e))?;
            &mut file_out
        }
        None => ctx.out,
    };

    engine::run(&Bytes, mode, input, out).map_err(|f| match f {
        Failure::Read(e) => path_error(in_name, &e),
        Failure::Write(e) => path_error(out_name, &e),
    })
}

/// `<path>: <strerror>`, as err(3) prints it.
fn path_error(path: &OsStr, e: &std::io::Error) -> AppError {
    AppError::Other(format!("{}: {}", path.to_string_lossy(), errno_message(e)))
}
