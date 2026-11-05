//! Increment 2: BSD getopt_long(3) argument handling, the obsolete -NUM/+NUM
//! forms, strtonum(3) checks and err(3)-style messages. Every expectation
//! here was observed from macOS /usr/bin/uniq.

mod common;

use std::path::Path;
use std::process::Output;

const USAGE: &str = "usage: runiq [-cdiu] [-D[septype]] [-f fields] [-s chars] [input [output]]\n";
const AB: &str = "a\na\nb\nb\n";

fn run_in(dir: &Path, args: &[&str], stdin: &str) -> Output {
    common::cmd(dir)
        .current_dir(dir)
        .env_remove("POSIXLY_CORRECT")
        .args(args)
        .write_stdin(stdin)
        .output()
        .unwrap()
}

fn run(args: &[&str], stdin: &str) -> Output {
    let dir = tempfile::tempdir().unwrap();
    run_in(dir.path(), args, stdin)
}

fn ok(args: &[&str], stdin: &str) -> String {
    let out = run(args, stdin);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    String::from_utf8(out.stdout).unwrap()
}

/// Exit 1, nothing on stdout, exactly `stderr`.
fn fails(args: &[&str], stderr: &str) {
    let out = run(args, "");
    assert_eq!(out.status.code(), Some(1), "{args:?}");
    assert!(out.stdout.is_empty(), "{args:?}");
    assert_eq!(String::from_utf8_lossy(&out.stderr), stderr, "{args:?}");
}

fn usage_with(message: &str) -> String {
    format!("runiq: {message}\n{USAGE}")
}

#[test]
fn short_options_cluster_and_take_attached_or_separate_values() {
    assert_eq!(ok(&["-cu"], "a\na\nb\n"), "   1 b\n");
    assert_eq!(ok(&["-cf1"], "1 a\n2 a\n"), "   2 1 a\n");
    assert_eq!(ok(&["-cf", "1"], "1 a\n2 a\n"), "   2 1 a\n");
    // "-0" is rewritten to "-f0" before getopt runs, then taken as -s's value.
    fails(
        &["-s", "-0"],
        "runiq: character skip value is invalid: -f0\n",
    );
    fails(&["-cf"], &usage_with("option requires an argument -- f"));
    fails(&["-s"], &usage_with("option requires an argument -- s"));
}

#[test]
fn all_repeated_value_must_be_attached_and_is_case_insensitive() {
    assert_eq!(ok(&["-Dprepend"], AB), "\na\na\n\nb\nb\n");
    assert_eq!(ok(&["-cDprepend"], "a\na\n"), "\n   1 a\n   2 a\n");
    assert_eq!(ok(&["-DNoNe"], AB), AB);
    assert_eq!(ok(&["--all-repeated=SEPARATE"], AB), "a\na\n\nb\nb\n");
    assert_eq!(ok(&["--all=separate"], AB), "a\na\n\nb\nb\n");
    assert_eq!(ok(&["-D", "-"], AB), AB);
    fails(&["-Dp"], USAGE);
    fails(&["-Dxyz"], USAGE);
    fails(&["--all-repeated="], USAGE);
    fails(&["-D-"], USAGE);
    // A separate word is an operand, never the value.
    fails(
        &["-D", "separate"],
        "runiq: separate: No such file or directory\n",
    );
    fails(
        &["--all-repeated", "prepend"],
        "runiq: prepend: No such file or directory\n",
    );
}

#[test]
fn long_options_abbreviate_and_take_values() {
    assert_eq!(ok(&["--co", "--uniq"], "a\na\nb\n"), "   1 b\n");
    assert_eq!(ok(&["--skip-f", "1", "-c"], "1 a\n2 a\n"), "   2 1 a\n");
    assert_eq!(ok(&["--skip-c=1", "-c"], "xa\nya\n"), "   2 xa\n");
    assert_eq!(ok(&["--unique", "--repeated"], "a\na\n"), "a\n");
    fails(&["--s", "1"], &usage_with("option `--s' is ambiguous"));
    fails(&["--skip"], &usage_with("option `--skip' is ambiguous"));
    fails(&["--=3"], &usage_with("option `--' is ambiguous"));
    fails(&["--nope"], &usage_with("unrecognized option `--nope'"));
    fails(&["--nope=3"], &usage_with("unrecognized option `--nope=3'"));
    fails(&["--he"], &usage_with("unrecognized option `--he'"));
    fails(&["---x"], &usage_with("unrecognized option `---x'"));
    fails(
        &["--ALL-REPEATED"],
        &usage_with("unrecognized option `--ALL-REPEATED'"),
    );
    fails(
        &["--count=3"],
        &usage_with("option `--count' doesn't allow an argument"),
    );
    fails(
        &["--cou="],
        &usage_with("option `--cou' doesn't allow an argument"),
    );
    fails(
        &["--skip-fields"],
        &usage_with("option `--skip-fields' requires an argument"),
    );
}

#[test]
fn a_dash_inside_a_cluster_starts_a_long_option() {
    assert_eq!(ok(&["-c-count"], AB), "   2 a\n   2 b\n");
    assert_eq!(ok(&["-c-skip-f", "1"], AB), "   4 a\n");
    fails(&["-c-x"], &usage_with("unrecognized option `--x'"));
    fails(
        &["-c--count"],
        &usage_with("unrecognized option `---count'"),
    );
    fails(&["-c-"], USAGE);
    fails(&["-cu-"], USAGE);
}

#[test]
fn invalid_option_characters() {
    fails(&["-x"], &usage_with("invalid option -- x"));
    fails(&["-x-"], &usage_with("invalid option -- x"));
    fails(&["-:"], &usage_with("invalid option -- :"));
    fails(&["-c+"], &usage_with("invalid option -- +"));
    fails(&["-h"], &usage_with("invalid option -- h"));
    fails(&["-V"], &usage_with("invalid option -- V"));
    let dir = tempfile::tempdir().unwrap();
    let out = common::cmd(dir.path())
        .env("POSIXLY_CORRECT", "")
        .arg("-x")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        usage_with("illegal option -- x")
    );
}

#[test]
fn operands_end_option_parsing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("in.txt"), "x\nx\n").unwrap();
    let out = run_in(dir.path(), &["in.txt", "-c"], "");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("-c")).unwrap(),
        "x\n"
    );

    fails(&["--", "-c"], "runiq: -c: No such file or directory\n");
    fails(&["-c", "+"], "runiq: +: No such file or directory\n");
    assert_eq!(ok(&["--"], "a\na\n"), "a\n");
    assert_eq!(ok(&["-c", "--", "-"], "a\na\n"), "   2 a\n");
    assert_eq!(ok(&["--count", "--"], "a\na\n"), "   2 a\n");
    fails(&["a", "b", "c"], USAGE);
    fails(&["-c", "a", "b", "c"], USAGE);
}

#[test]
fn obsolete_number_forms() {
    assert_eq!(ok(&["-1", "-c"], "a b\nc b\n"), "   2 a b\n");
    assert_eq!(ok(&["+1", "-c"], "ab\ncb\n"), "   2 ab\n");
    fails(&["-c1"], &usage_with("invalid option -- 1"));
    fails(&["-1x"], "runiq: field skip value is invalid: 1x\n");
    fails(
        &["-c", "+1x"],
        "runiq: character skip value is invalid: 1x\n",
    );
    // The rewrite also hits option values before the first operand...
    fails(&["-f", "-1"], "runiq: field skip value is invalid: -f1\n");
    fails(&["-f", "+5"], "runiq: field skip value is invalid: -s5\n");
    // ...and stops at the first word that is not an option.
    fails(&["-f", "3", "-1"], &usage_with("invalid option -- 1"));
}

#[test]
fn skip_values_follow_strtonum() {
    assert_eq!(ok(&["-f", " 1", "-c"], "1 a\n2 a\n"), "   2 1 a\n");
    assert_eq!(ok(&["-f+1", "-c"], "1 a\n2 a\n"), "   2 1 a\n");
    assert_eq!(ok(&["-s", " +1", "-c"], "xa\nya\n"), "   2 xa\n");
    assert_eq!(
        ok(&["-s", "00000000000000000000000000001", "-c"], "xa\nya\n"),
        "   2 xa\n"
    );
    assert_eq!(ok(&["-s", "2147483647"], "x\n"), "x\n");
    fails(&["-f", "abc"], "runiq: field skip value is invalid: abc\n");
    fails(&["-f", ""], "runiq: field skip value is invalid: \n");
    fails(&["-f", "5 "], "runiq: field skip value is invalid: 5 \n");
    fails(
        &["-s", "0x3"],
        "runiq: character skip value is invalid: 0x3\n",
    );
    fails(&["-s", "-"], "runiq: character skip value is invalid: -\n");
    fails(
        &["-f", " -5"],
        "runiq: field skip value is too small:  -5\n",
    );
    fails(
        &["-s", "2147483648"],
        "runiq: character skip value is too large: 2147483648\n",
    );
    fails(
        &["-s", "99999999999999999999"],
        "runiq: character skip value is too large: 99999999999999999999\n",
    );
    fails(
        &["-s", " -99999999999999999999"],
        "runiq: character skip value is too small:  -99999999999999999999\n",
    );
    fails(
        &["-s", "-99999999999999999999"],
        "runiq: character skip value is invalid: -f99999999999999999999\n",
    );
}

#[test]
fn errors_are_reported_in_argument_order() {
    fails(
        &["-f", "abc", "-x"],
        "runiq: field skip value is invalid: abc\n",
    );
    fails(&["-x", "-f", "abc"], &usage_with("invalid option -- x"));
    fails(&["-Dbad", "-f", "abc"], USAGE);
    fails(
        &["-f", "abc", "a", "b", "c"],
        "runiq: field skip value is invalid: abc\n",
    );
}

#[test]
fn last_value_wins() {
    assert_eq!(
        ok(&["-f", "9", "-f", "0", "-c"], "a\nb\n"),
        "   1 a\n   1 b\n"
    );
    assert_eq!(ok(&["-Dprepend", "-Dnone"], "a\na\n"), "a\na\n");
}

#[test]
fn help_and_version_are_long_only_extensions() {
    let help = ok(&["--help"], "");
    assert!(
        help.contains("Usage: runiq [OPTIONS] [INPUT] [OUTPUT]"),
        "{help}"
    );
    fails(&["--hel"], &usage_with("unrecognized option `--hel'"));
    assert!(ok(&["--version"], "").starts_with("runiq "));
    assert!(ok(&["-c", "--version"], "").starts_with("runiq "));
}

#[test]
fn messages_use_the_invoked_program_name() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("uniq");
    std::os::unix::fs::symlink(assert_cmd::cargo::cargo_bin!("runiq"), &link).unwrap();
    let out = std::process::Command::new(&link)
        .current_dir(dir.path())
        .env_remove("POSIXLY_CORRECT")
        .args(["-x"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "uniq: invalid option -- x\n\
         usage: uniq [-cdiu] [-D[septype]] [-f fields] [-s chars] [input [output]]\n"
    );
    let out = std::process::Command::new(&link)
        .current_dir(dir.path())
        .arg("nofile")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "uniq: nofile: No such file or directory\n"
    );
}
