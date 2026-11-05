//! `schema`: the machine-readable interface, rendered from `cli.toml` at build time.

use crate::ctx::Ctx;
use crate::error::AppError;

pub fn run(ctx: &mut Ctx) -> Result<(), AppError> {
    writeln!(ctx.out, "{}", crate::generated::SCHEMA_JSON)?;
    Ok(())
}
