//! Conformance: the option tables in src/getopt.rs and the prebuilt --help must
//! match cli.toml, and the binary must stay free of heavy dependencies.
//!
//! When this fails, decide which side is wrong. cli.toml is the approved
//! design: change the code unless the spec itself needs to change.

mod common;

use common::clap_model;
use common::spec::FlagKind;
use std::collections::BTreeSet;
use uniqr::getopt::{HasArg, LONG, SHORT};

#[test]
fn cli_toml_is_valid() {
    let problems = common::spec().validate();
    assert!(
        problems.is_empty(),
        "cli.toml problems:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn cli_name_matches_package() {
    assert_eq!(common::spec().cli.name, uniqr::NAME);
}

#[test]
fn prebuilt_help_matches_cli_toml() {
    let rendered = clap_model::render(common::spec(), "help").unwrap();
    assert_eq!(
        String::from_utf8(rendered).unwrap(),
        uniqr::HELP,
        "src/help.txt is stale; run `just snapshots`"
    );
}

#[test]
fn getopt_tables_match_cli_toml() {
    let spec = common::spec();
    let mut long: BTreeSet<&str> = LONG.iter().map(|(name, _, _)| *name).collect();
    let mut short: BTreeSet<u8> = SHORT.iter().map(|(c, _)| *c).collect();
    assert!(long.remove("help") && long.remove("version"));
    for f in &spec.flags {
        let (_, has_arg, value) = LONG
            .iter()
            .find(|(name, _, _)| *name == f.long)
            .unwrap_or_else(|| panic!("--{} is not in getopt's LONG", f.long));
        let want = match (f.kind, &f.default_missing) {
            (FlagKind::Switch, _) => HasArg::No,
            (_, Some(_)) => HasArg::Optional,
            _ => HasArg::Required,
        };
        assert!(*has_arg == want, "--{} takes its value wrongly", f.long);
        let letter = u8::try_from(f.short.expect("every flag has a short form")).unwrap();
        assert_eq!(
            *value, letter,
            "--{} is not the same option as -{}",
            f.long, letter as char
        );
        let (_, has_short_arg) = SHORT
            .iter()
            .find(|(c, _)| *c == letter)
            .unwrap_or_else(|| panic!("-{} is not in getopt's SHORT", letter as char));
        assert!(
            *has_short_arg == want,
            "-{} takes its value wrongly",
            letter as char
        );
        long.remove(f.long.as_str());
        short.remove(&letter);
    }
    assert!(long.is_empty(), "not in cli.toml: {long:?}");
    assert!(short.is_empty(), "not in cli.toml: {short:?}");
}

#[test]
fn clap_model_is_consistent() {
    // Panics on clashes clap can't resolve.
    clap_model::command(common::spec()).debug_assert();
}

#[test]
fn documentation_renders() {
    let spec = common::spec();
    let text = |what: &str| String::from_utf8(clap_model::render(spec, what).unwrap()).unwrap();
    assert!(text("man").contains(".TH"));
    assert!(text("complete-zsh").contains(uniqr::NAME));
    let doc: serde_json::Value = serde_json::from_str(&text("schema")).unwrap();
    assert_eq!(doc["name"], uniqr::NAME);
    assert!(clap_model::render(spec, "nope").is_none());
    assert!(clap_model::render(spec, "complete-tcsh").is_none());
}

#[test]
fn the_binary_only_depends_on_libc_and_memchr() {
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap();
    let mut section = "";
    let mut deps = Vec::new();
    for line in manifest.lines() {
        if line.starts_with('[') {
            section = line;
        } else if section == "[dependencies]"
            && let Some((name, _)) = line.split_once(" = ")
        {
            deps.push(name);
        }
    }
    assert_eq!(
        deps,
        ["libc", "memchr"],
        "a new runtime dependency grows the binary; is it worth it?"
    );
}
