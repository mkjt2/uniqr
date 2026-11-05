//! Conformance: the clap definitions in src/cli.rs must match cli.toml exactly,
//! and the error catalog in src/error.rs must match its exit codes.
//!
//! When this fails, decide which side is wrong. cli.toml is the approved
//! design: change the code unless the spec itself needs to change (and then
//! re-approve it with the user).

mod common;

use clap::{ArgAction, CommandFactory};
use common::spec::{Arg, Flag, FlagKind, Spec};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use uniqr::cli::Cli;

fn kind_of(arg: &clap::Arg) -> Option<FlagKind> {
    match arg.get_action() {
        ArgAction::SetTrue | ArgAction::SetFalse => Some(FlagKind::Switch),
        ArgAction::Count => Some(FlagKind::Count),
        ArgAction::Set => Some(FlagKind::Value),
        ArgAction::Append => Some(FlagKind::Values),
        _ => None,
    }
}

fn possible_values(arg: &clap::Arg) -> Vec<String> {
    arg.get_possible_values()
        .iter()
        .filter(|p| !p.is_hide_set())
        .map(|p| p.get_name().to_string())
        .collect()
}

fn defaults(arg: &clap::Arg) -> Option<String> {
    let vals: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|v| v.to_string_lossy().into_owned())
        .collect();
    (!vals.is_empty()).then(|| vals.join(","))
}

fn value_name(arg: &clap::Arg) -> Option<String> {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map(|n| n.to_string())
}

fn check_flag(problems: &mut Vec<String>, owner: &str, flag: &Flag, arg: &clap::Arg) {
    let mut diff = |field: &str, spec: String, code: String| {
        if spec != code {
            problems.push(format!(
                "{owner} --{}: {field} is {code} in code but {spec} in cli.toml",
                flag.long
            ));
        }
    };
    diff(
        "long",
        format!("{:?}", Some(&flag.long)),
        format!("{:?}", arg.get_long()),
    );
    diff(
        "short",
        format!("{:?}", flag.short),
        format!("{:?}", arg.get_short()),
    );
    diff(
        "kind",
        format!("{:?}", Some(flag.kind)),
        format!("{:?}", kind_of(arg)),
    );
    diff(
        "global",
        flag.global.to_string(),
        arg.is_global_set().to_string(),
    );
    diff(
        "required",
        flag.required.to_string(),
        arg.is_required_set().to_string(),
    );
    diff(
        "env",
        format!("{:?}", flag.env),
        format!(
            "{:?}",
            arg.get_env().map(|e| e.to_string_lossy().into_owned())
        ),
    );
    if matches!(flag.kind, FlagKind::Value | FlagKind::Values) {
        diff(
            "default",
            format!("{:?}", flag.default),
            format!("{:?}", defaults(arg)),
        );
        diff(
            "possible_values",
            format!("{:?}", flag.possible_values),
            format!("{:?}", possible_values(arg)),
        );
        let expected = flag
            .value_name
            .clone()
            .unwrap_or_else(|| flag.id.to_uppercase());
        diff(
            "value_name",
            format!("{:?}", Some(expected)),
            format!("{:?}", value_name(arg)),
        );
        // clap has no getter for default-missing values; its Debug output does show them.
        let optional = arg.get_num_args().is_some_and(|r| r.min_values() == 0);
        let debug = format!("{arg:?}");
        let missing = flag
            .default_missing
            .as_ref()
            .map(|v| debug.contains(&format!("default_missing_vals: [{v:?}]")));
        diff(
            "optional value (default_missing)",
            flag.default_missing.is_some().to_string(),
            optional.to_string(),
        );
        if let Some(found) = missing {
            diff("default_missing value", "true".into(), found.to_string());
            diff(
                "require_equals",
                "true".into(),
                arg.is_require_equals_set().to_string(),
            );
        }
    }
    let mut aliases: Vec<String> = arg
        .get_all_aliases()
        .unwrap_or_default()
        .into_iter()
        .map(String::from)
        .collect();
    aliases.sort();
    let mut spec_aliases = flag.aliases.clone();
    spec_aliases.sort();
    diff(
        "aliases",
        format!("{spec_aliases:?}"),
        format!("{aliases:?}"),
    );
    let mut shorts = arg.get_all_short_aliases().unwrap_or_default();
    shorts.sort();
    let mut spec_shorts = flag.short_aliases.clone();
    spec_shorts.sort();
    diff(
        "short_aliases",
        format!("{spec_shorts:?}"),
        format!("{shorts:?}"),
    );
}

fn check_positional(problems: &mut Vec<String>, owner: &str, spec: &Arg, arg: &clap::Arg) {
    let mut diff = |field: &str, s: String, c: String| {
        if s != c {
            problems.push(format!(
                "{owner} <{}>: {field} is {c} in code but {s} in cli.toml",
                spec.id
            ));
        }
    };
    let multiple = matches!(arg.get_action(), ArgAction::Append)
        || arg.get_num_args().is_some_and(|r| r.max_values() > 1);
    diff(
        "required",
        spec.required.to_string(),
        arg.is_required_set().to_string(),
    );
    diff("multiple", spec.multiple.to_string(), multiple.to_string());
    diff(
        "default",
        format!("{:?}", spec.default),
        format!("{:?}", defaults(arg)),
    );
    diff(
        "possible_values",
        format!("{:?}", spec.possible_values),
        format!("{:?}", possible_values(arg)),
    );
    let expected = spec
        .value_name
        .clone()
        .unwrap_or_else(|| spec.id.to_uppercase());
    diff(
        "value_name",
        format!("{:?}", Some(expected)),
        format!("{:?}", value_name(arg)),
    );
}

fn check_args(
    problems: &mut Vec<String>,
    owner: &str,
    cmd: &clap::Command,
    flags: &[Flag],
    positionals: &[Arg],
) {
    let code_args: Vec<&clap::Arg> = cmd
        .get_arguments()
        .filter(|a| !matches!(a.get_id().as_str(), "help" | "version"))
        .collect();
    let code_ids: BTreeSet<String> = code_args.iter().map(|a| a.get_id().to_string()).collect();
    let spec_ids: BTreeSet<String> = flags
        .iter()
        .map(|f| f.id.clone())
        .chain(positionals.iter().map(|a| a.id.clone()))
        .collect();
    for missing in spec_ids.difference(&code_ids) {
        problems.push(format!(
            "{owner}: `{missing}` is in cli.toml but not in code"
        ));
    }
    for extra in code_ids.difference(&spec_ids) {
        problems.push(format!("{owner}: `{extra}` is in code but not in cli.toml"));
    }
    for arg in code_args {
        let id = arg.get_id().as_str();
        if let Some(flag) = flags.iter().find(|f| f.id == id) {
            if arg.is_positional() {
                problems.push(format!(
                    "{owner}: `{id}` is positional in code but a flag in cli.toml"
                ));
            } else {
                check_flag(problems, owner, flag, arg);
            }
        } else if let Some(pos) = positionals.iter().find(|p| p.id == id) {
            if arg.is_positional() {
                check_positional(problems, owner, pos, arg);
            } else {
                problems.push(format!(
                    "{owner}: `{id}` is a flag in code but positional in cli.toml"
                ));
            }
        }
    }
}

fn check_command(problems: &mut Vec<String>, spec: &Spec, cmd: &clap::Command, path: &str) {
    if !path.is_empty() {
        let Some(entry) = spec.command(path) else {
            problems.push(format!("command `{path}` is in code but not in cli.toml"));
            return;
        };
        check_args(
            problems,
            &format!("`{path}`"),
            cmd,
            &entry.flags,
            &entry.args,
        );
    }
    let code_children: BTreeSet<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    let spec_children: BTreeSet<String> = spec
        .commands
        .iter()
        .filter_map(|c| match c.path.rsplit_once(' ') {
            Some((parent, name)) if parent == path => Some(name.to_string()),
            None if path.is_empty() => Some(c.path.clone()),
            _ => None,
        })
        .collect();
    for missing in spec_children.difference(&code_children) {
        let full = if path.is_empty() {
            missing.clone()
        } else {
            format!("{path} {missing}")
        };
        problems.push(format!("command `{full}` is in cli.toml but not in code"));
    }
    for sub in cmd.get_subcommands() {
        let child = if path.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{path} {}", sub.get_name())
        };
        if !spec_children.contains(sub.get_name()) {
            problems.push(format!("command `{child}` is in code but not in cli.toml"));
            continue;
        }
        check_command(problems, spec, sub, &child);
    }
}

#[test]
fn clap_definitions_match_cli_toml() {
    let spec = common::spec();
    let root = Cli::command();
    let mut problems = Vec::new();
    check_args(&mut problems, "<root>", &root, &spec.flags, &spec.args);
    check_command(&mut problems, spec, &root, "");
    assert!(
        problems.is_empty(),
        "src/cli.rs and cli.toml disagree:\n  {}",
        problems.join("\n  ")
    );
}

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
    assert_eq!(common::spec().cli.name, env!("CARGO_PKG_NAME"));
}

#[test]
fn error_catalog_matches_exit_codes() {
    let declared = common::spec().error_catalog();
    let code: BTreeSet<(String, i32)> = uniqr::error::EXIT_CODES
        .iter()
        .map(|(n, c)| ((*n).to_string(), *c))
        .collect();
    assert_eq!(
        code, declared,
        "src/error.rs EXIT_CODES must match cli.toml: usage = 2, error = [cli].error_exit_code, \
         plus every [[exit_code]]"
    );
}

#[test]
fn final_command_is_consistent() {
    // Panics on clashes clap can't resolve, e.g. a flag using -h while
    // `help_flags` still claims it.
    uniqr::cli::command().debug_assert();
}

#[test]
fn help_and_version_flags_match_cli_toml() {
    use common::spec::split_flags;
    let spec = common::spec();
    let cmd = uniqr::cli::command();
    for (id, flags, auto) in [
        ("help", &spec.cli.help_flags, ["-h", "--help"]),
        ("version", &spec.cli.version_flags, ["-V", "--version"]),
    ] {
        let mut probe = cmd.clone();
        probe.build();
        let arg = probe.get_arguments().find(|a| a.get_id() == id);
        let (short, long) = split_flags(flags);
        if flags.as_slice() == auto {
            continue; // clap's built-in flag
        }
        assert_eq!(arg.and_then(|a| a.get_short()), short, "{id} short flag");
        assert_eq!(
            arg.and_then(|a| a.get_long()).map(String::from),
            long,
            "{id} long flag"
        );
    }
}

#[test]
fn every_flag_and_command_has_help_text_applied() {
    let cmd = uniqr::cli::command();
    fn walk(cmd: &clap::Command, path: &str, problems: &mut Vec<String>) {
        if cmd.get_about().is_none() {
            problems.push(format!("`{path}` has no about"));
        }
        for arg in cmd.get_arguments() {
            if !matches!(arg.get_id().as_str(), "help" | "version") && arg.get_help().is_none() {
                problems.push(format!("`{path}` arg `{}` has no help", arg.get_id()));
            }
        }
        for sub in cmd.get_subcommands() {
            walk(sub, format!("{path} {}", sub.get_name()).trim(), problems);
        }
    }
    let mut problems = Vec::new();
    walk(&cmd, "", &mut problems);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn build_time_tables_match_cli_toml() {
    use uniqr::generated as g;
    let spec = common::spec();
    assert_eq!(g::NAME, spec.cli.name);
    assert_eq!(g::ABOUT, spec.cli.about);
    assert_eq!(g::LONG_ABOUT, spec.cli.long_about.as_deref());
    assert_eq!(g::ROOT_AFTER_LONG_HELP, spec.root_after_help());
    assert_eq!(g::KIT_JSON, spec.kit.json);
    assert_eq!(g::ERROR_EXIT_CODE, spec.cli.error_exit_code);
    assert_eq!(
        g::SCHEMA_JSON,
        spec.schema_document(env!("CARGO_PKG_VERSION")).to_string()
    );
    let root: Vec<&str> = g::ROOT_ARG_HELP.iter().map(|(id, _)| *id).collect();
    let spec_root: Vec<&str> = spec
        .flags
        .iter()
        .map(|f| f.id.as_str())
        .chain(spec.args.iter().map(|a| a.id.as_str()))
        .collect();
    assert_eq!(root, spec_root);
    let paths: Vec<&str> = g::COMMANDS.iter().map(|c| c.path).collect();
    let spec_paths: Vec<&str> = spec.commands.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, spec_paths);
    for (text, cmd) in g::COMMANDS.iter().zip(&spec.commands) {
        assert_eq!(text.about, cmd.about, "`{}` about", cmd.path);
        assert_eq!(
            text.after_help.is_some(),
            !cmd.examples.is_empty(),
            "`{}` examples",
            cmd.path
        );
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Problems in a runtime source file that would embed or parse cli.toml.
fn runtime_spec_parsing(file: &Path, text: &str) -> Vec<String> {
    let name = file.file_name().unwrap().to_string_lossy();
    let mut problems: Vec<String> = ["cli.toml\")", "Spec::get", "Spec::parse", "mod spec;"]
        .into_iter()
        .filter(|needle| text.contains(needle))
        .map(|needle| format!("{}: `{needle}`", file.display()))
        .collect();
    if name != "config.rs" && text.contains("toml::from_str") {
        problems.push(format!("{}: `toml::from_str`", file.display()));
    }
    problems
}

#[test]
fn runtime_code_does_not_parse_cli_toml() {
    // build.rs renders what the binary needs from cli.toml (src/generated.rs);
    // src/spec.rs is the model for build.rs and the tests, not a library module.
    let mut files = Vec::new();
    rust_files(Path::new("src"), &mut files);
    let problems: Vec<String> = files
        .iter()
        .filter(|f| !f.ends_with("spec.rs"))
        .flat_map(|f| runtime_spec_parsing(f, &std::fs::read_to_string(f).unwrap()))
        .collect();
    assert!(
        problems.is_empty(),
        "the binary must not embed or parse cli.toml:\n  {}",
        problems.join("\n  ")
    );
    // The guard itself catches a re-added runtime parse.
    assert!(!runtime_spec_parsing(Path::new("src/lib.rs"), "let s = Spec::get();").is_empty());
    assert!(!runtime_spec_parsing(Path::new("src/x.rs"), "toml::from_str(t)").is_empty());
    assert!(runtime_spec_parsing(Path::new("src/config.rs"), "toml::from_str(t)").is_empty());
}

#[test]
fn toml_is_a_runtime_dependency_only_for_the_config_kit() {
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap();
    let mut section = "";
    for line in manifest.lines() {
        if line.starts_with('[') {
            section = line;
        } else if section == "[dependencies]" && line.starts_with("toml ") {
            assert!(
                line.ends_with("# kit:config"),
                "toml must stay out of the binary unless the config kit needs it: {line}"
            );
        }
    }
}
