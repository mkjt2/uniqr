//! The `cli.toml` model (`src/spec.rs`): parsing, validation and the text
//! build.rs renders into the binary.

mod common;

use common::spec::{Example, Spec, examples_section};

fn spec_with(extra: &str) -> Spec {
    Spec::parse(&format!(
        "schema_version = 1\n[cli]\nname = \"x\"\nabout = \"y\"\n{extra}\n"
    ))
    .unwrap()
}

#[test]
fn cli_toml_parses_and_validates() {
    let spec = common::spec();
    assert_eq!(spec.cli.name, env!("CARGO_PKG_NAME"));
    assert_eq!(spec.validate(), Vec::<String>::new());
}

#[test]
fn validate_reports_problems() {
    let spec = Spec::parse(
        r#"
        schema_version = 2
        [cli]
        name = "x"
        about = " "
        error_exit_code = 9
        [[exit_code]]
        code = 0
        name = "zero"
        description = "out of range"
        [[exit_code]]
        code = 7
        name = "a"
        description = "first"
        [[exit_code]]
        code = 7
        name = "b"
        description = "second"
        [[arg]]
        id = "file"
        help = "mixed with commands"
        [[case]]
        name = "c"
        stdout = "x"
        stdout_file = "x.out"
        exit_code = 99
        [[case]]
        name = "c"
        [[flag]]
        id = "f"
        long = "f"
        kind = "switch"
        value_name = "V"
        help = ""
        [[command]]
        path = "a b"
        about = ""
        mutates = true
        exit_codes = [99]
        [[command.arg]]
        id = "x"
        help = ""
        [[command]]
        path = "a b"
        about = "again"
        "#,
    )
    .unwrap();
    let problems = spec.validate().join("\n");
    for needle in [
        "schema_version",
        "cli.about",
        "exit code 7 declared twice",
        "must be in 1..=125",
        "cannot be mixed",
        "error_exit_code 9",
        "case `c` declared twice",
        "both stdout and stdout_file",
        "undeclared exit code 99",
        "declared twice",
        "empty about",
        "no parent entry `a`",
        "--dry-run",
        "arg `x`",
        "empty help",
        "takes no value",
    ] {
        assert!(
            problems.contains(needle),
            "missing `{needle}` in:\n{problems}"
        );
    }
}

#[test]
fn unknown_fields_are_rejected() {
    let err =
        Spec::parse("schema_version = 1\n[cli]\nname='x'\nabout='y'\ntypo = 1\n").unwrap_err();
    assert!(err.to_string().contains("typo"));
}

#[test]
fn exit_codes_are_sorted_and_include_builtins() {
    let codes: Vec<i32> = common::spec()
        .all_exit_codes()
        .iter()
        .map(|c| c.code)
        .collect();
    let mut sorted = codes.clone();
    sorted.sort();
    assert_eq!(codes, sorted);
    assert!(codes.contains(&0) && codes.contains(&2) && codes.contains(&130));
}

#[test]
fn error_exit_code_moves_the_error_builtin() {
    let spec = spec_with("error_exit_code = 2");
    let codes: Vec<(i32, String)> = spec
        .all_exit_codes()
        .into_iter()
        .map(|c| (c.code, c.name))
        .collect();
    assert!(!codes.iter().any(|(_, name)| name == "error"), "{codes:?}");
    assert!(spec.error_catalog().contains(&("error".into(), 2)));
}

#[test]
fn examples_section_lists_description_then_command() {
    let examples = [
        Example {
            cmd: "x a".into(),
            description: "Do a".into(),
        },
        Example {
            cmd: "x \"b\"".into(),
            description: "Quote b".into(),
        },
    ];
    assert_eq!(
        examples_section(&examples),
        "Examples:\n  # Do a\n  $ x a\n  # Quote b\n  $ x \"b\""
    );
}

#[test]
fn root_help_mentions_schema_only_when_there_is_a_schema_command() {
    let spec = spec_with("");
    assert!(!spec.root_after_help().contains("schema"));
    assert!(spec.root_after_help().starts_with("Exit codes:"));
    let spec = spec_with("[[command]]\npath = \"schema\"\nabout = \"Print the schema\"");
    assert!(
        spec.root_after_help()
            .ends_with("Run `x schema` for the full machine-readable interface.")
    );
}

#[test]
fn root_help_puts_examples_before_exit_codes() {
    let spec = spec_with("[[cli.examples]]\ncmd = \"x go\"\ndescription = \"Go\"");
    let text = spec.root_after_help();
    assert!(
        text.starts_with("Examples:\n  # Go\n  $ x go\n\nExit codes:\n"),
        "{text}"
    );
    assert!(text.contains("    0  Success\n"), "{text}");
}

#[test]
fn schema_document_describes_the_interface() {
    let single = spec_with("").schema_document("1.2.3");
    assert_eq!(single["version"], "1.2.3");
    assert_eq!(single["name"], "x");
    assert_eq!(single["shape"], "single");
    let spec = spec_with("[[command]]\npath = \"go\"\nabout = \"Go\"");
    let doc = spec.schema_document("1.2.3");
    assert_eq!(doc["shape"], "subcommands");
    assert_eq!(doc["commands"][0]["usage"], "x go");
    assert_eq!(doc["error_schema"]["required"][0], "error");
}
