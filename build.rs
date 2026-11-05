//! Renders `cli.toml` into static tables (`OUT_DIR/spec_gen.rs`, included by
//! `src/generated.rs`), so the binary never embeds or parses the spec.

#[allow(dead_code)] // the model also serves the tests
#[path = "src/spec.rs"]
mod spec;

use std::fmt::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=cli.toml");
    println!("cargo:rerun-if-changed=src/spec.rs");
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let text = std::fs::read_to_string(Path::new(&dir).join("cli.toml")).expect("read cli.toml");
    let spec = spec::Spec::parse(&text).unwrap_or_else(|e| panic!("cli.toml is invalid: {e}"));
    let out = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("spec_gen.rs");
    std::fs::write(out, render(&spec)).expect("write spec_gen.rs");
}

/// Rust source for the tables.
fn render(spec: &spec::Spec) -> String {
    let cli = &spec.cli;
    let mut out = String::new();
    let mut constant = |name: &str, ty: &str, value: String| {
        writeln!(out, "pub const {name}: {ty} = {value};").unwrap();
    };
    constant("NAME", "&str", lit(&cli.name));
    constant("ABOUT", "&str", lit(&cli.about));
    constant("LONG_ABOUT", "Option<&str>", lit(cli.long_about.as_deref()));
    constant("ROOT_AFTER_LONG_HELP", "&str", lit(spec.root_after_help()));
    constant(
        "ROOT_ARG_HELP",
        "&[(&str, &str)]",
        arg_help(&spec.flags, &spec.args),
    );
    constant(
        "HELP_FLAGS",
        "Option<FlagNames>",
        flag_names(&cli.help_flags, ["-h", "--help"]),
    );
    constant(
        "VERSION_FLAGS",
        "Option<FlagNames>",
        flag_names(&cli.version_flags, ["-V", "--version"]),
    );
    constant("KIT_JSON", "bool", spec.kit.json.to_string());
    constant(
        "ERROR_STYLE",
        "ErrorStyle",
        format!("ErrorStyle::{:?}", cli.error_style),
    );
    constant("ERROR_EXIT_CODE", "i32", cli.error_exit_code.to_string());
    constant("USAGE_EXIT_CODE", "i32", spec::USAGE_EXIT_CODE.to_string());
    constant(
        "SCHEMA_JSON",
        "&str",
        lit(spec.schema_document(env!("CARGO_PKG_VERSION")).to_string()),
    );
    let commands: Vec<String> = spec
        .commands
        .iter()
        .map(|c| {
            let after_help = (!c.examples.is_empty()).then(|| spec::examples_section(&c.examples));
            format!(
                "CommandText {{ path: {:?}, about: {:?}, long_about: {:?}, after_help: {:?}, arg_help: {} }}",
                c.path,
                c.about,
                c.long_about.as_deref(),
                after_help.as_deref(),
                arg_help(&c.flags, &c.args),
            )
        })
        .collect();
    constant(
        "COMMANDS",
        "&[CommandText]",
        format!("&[{}]", commands.join(", ")),
    );
    out
}

/// `value` as a Rust literal: `{:?}` writes strings, chars and options as
/// valid Rust source.
fn lit(value: impl std::fmt::Debug) -> String {
    format!("{value:?}")
}

/// `&[(id, help)]` for flags, then positionals.
fn arg_help(flags: &[spec::Flag], args: &[spec::Arg]) -> String {
    let pairs: Vec<String> = flags
        .iter()
        .map(|f| (&f.id, &f.help))
        .chain(args.iter().map(|a| (&a.id, &a.help)))
        .map(|(id, help)| format!("({id:?}, {help:?})"))
        .collect();
    format!("&[{}]", pairs.join(", "))
}

/// `None` when `flags` are clap's own, else `Some((short, long))`.
fn flag_names(flags: &[String], clap_default: [&str; 2]) -> String {
    if flags == clap_default {
        return "None".into();
    }
    let (short, long) = spec::split_flags(flags);
    format!("Some(({short:?}, {:?}))", long.as_deref())
}
