//! Increment 1: the core engine. Operands, files, streaming, and the
//! -c/-d/-u/-D output state machine (byte-exact with macOS uniq).

mod common;

use std::process::Output;

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    common::cmd(dir.path())
        .current_dir(dir.path())
        .env("LC_ALL", "C")
        .args(args)
        .write_stdin(stdin.to_vec())
        .output()
        .unwrap()
}

fn stdout(args: &[&str], stdin: &[u8]) -> Vec<u8> {
    let out = run(args, stdin);
    assert!(
        out.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

const MIXED: &[u8] = b"a\na\nb\nc\nc\nc\nd\n";

#[test]
fn default_collapses_adjacent_duplicates_only() {
    assert_eq!(stdout(&[], b"a\na\nb\na\n"), b"a\nb\na\n");
}

#[test]
fn empty_input_prints_nothing() {
    for args in [&[][..], &["-c"], &["-d"], &["-u"], &["--all-repeated=none"]] {
        assert_eq!(stdout(args, b""), b"", "{args:?}");
    }
}

#[test]
fn count_pads_to_four_columns_and_widens() {
    assert_eq!(stdout(&["-c"], MIXED), b"   2 a\n   1 b\n   3 c\n   1 d\n");
    let many = "x\n".repeat(12345);
    assert_eq!(stdout(&["-c"], many.as_bytes()), b"12345 x\n");
}

#[test]
fn repeated_unique_and_combinations() {
    assert_eq!(stdout(&["-d"], MIXED), b"a\nc\n");
    assert_eq!(stdout(&["-u"], MIXED), b"b\nd\n");
    assert_eq!(stdout(&["-c", "-d"], MIXED), b"   2 a\n   3 c\n");
    assert_eq!(stdout(&["-c", "-u"], MIXED), b"   1 b\n   1 d\n");
    assert_eq!(stdout(&["-d", "-u"], MIXED), b"a\nc\n");
    assert_eq!(stdout(&["-c", "-d", "-u"], MIXED), b"");
}

#[test]
fn all_repeated_separators() {
    assert_eq!(stdout(&["--all-repeated"], MIXED), b"a\na\nc\nc\nc\n");
    assert_eq!(stdout(&["--all-repeated=none"], MIXED), b"a\na\nc\nc\nc\n");
    assert_eq!(
        stdout(&["--all-repeated=prepend"], MIXED),
        b"\na\na\n\nc\nc\nc\n"
    );
    assert_eq!(
        stdout(&["--all-repeated=separate"], MIXED),
        b"a\na\n\nc\nc\nc\n\n"
    );
    // No separator after a final group that ends the input.
    assert_eq!(
        stdout(&["--all-repeated=separate"], b"a\na\nb\nc\nc\n"),
        b"a\na\n\nc\nc\n"
    );
}

#[test]
fn all_repeated_interactions() {
    assert_eq!(stdout(&["-d", "--all-repeated"], b"a\na\nb\n"), b"a\na\n");
    assert_eq!(
        stdout(&["-c", "--all-repeated"], MIXED),
        b"   1 a\n   2 a\n   1 c\n   2 c\n   3 c\n"
    );
    assert_eq!(stdout(&["-u", "--all-repeated"], MIXED), b"a\na\nc\nc\nc\n");
}

#[test]
fn last_line_without_newline() {
    assert_eq!(stdout(&[], b"a\nb"), b"a\nb");
    assert_eq!(stdout(&["-c"], b"a\na"), b"   2 a\n");
    assert_eq!(stdout(&["-c"], b"a\nb"), b"   1 a\n   1 b");
    assert_eq!(stdout(&["--all-repeated"], b"a\na"), b"a\na");
}

#[test]
fn nul_truncates_output_and_comparison() {
    assert_eq!(stdout(&[], b"a\0b\nc\nc\0d\n"), b"ac\n");
    assert_eq!(stdout(&["-c"], b"x\x001\nx\0 2\ny\n"), b"   2 x   1 y\n");
}

#[test]
fn empty_lines_are_lines() {
    assert_eq!(stdout(&["-c"], b"\n\nx\n"), b"   2 \n   1 x\n");
}

#[test]
fn long_lines_and_many_lines_cross_buffer_boundaries() {
    let long = "z".repeat(3 << 20);
    let input = format!("{long}\n{long}\nq\n{long}");
    let want = format!("   2 {long}\n   1 q\n   1 {long}");
    assert_eq!(stdout(&["-c"], input.as_bytes()), want.as_bytes());

    let mut input = String::new();
    let mut want = String::new();
    for i in 0..200_000 {
        let n = i % 3 + 1;
        for _ in 0..n {
            input.push_str(&format!("line {i}\n"));
        }
        want.push_str(&format!("{n:>4} line {i}\n"));
    }
    assert_eq!(stdout(&["-c"], input.as_bytes()), want.as_bytes());
}

#[test]
fn input_and_output_operands() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("in.txt"), "x\nx\ny\n").unwrap();
    std::fs::write(dir.path().join("out.txt"), "old contents that are longer\n").unwrap();
    let out = common::cmd(dir.path())
        .current_dir(dir.path())
        .args(["-c", "in.txt", "out.txt"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("out.txt")).unwrap(),
        "   2 x\n   1 y\n"
    );
}

#[test]
fn dash_input_reads_stdin_and_output_is_created_for_empty_input() {
    let dir = tempfile::tempdir().unwrap();
    let out = common::cmd(dir.path())
        .current_dir(dir.path())
        .args(["-", "out.txt"])
        .write_stdin("")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(std::fs::read(dir.path().join("out.txt")).unwrap(), b"");
}

#[test]
fn missing_input_is_reported_before_output_is_touched() {
    let dir = tempfile::tempdir().unwrap();
    let out = common::cmd(dir.path())
        .current_dir(dir.path())
        .args(["nofile", "out.txt"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "runiq: nofile: No such file or directory\n"
    );
    assert!(!dir.path().join("out.txt").exists());
}

#[test]
fn unreadable_input_and_unwritable_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = common::cmd(dir.path())
        .current_dir(dir.path())
        .arg(".")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "runiq: .: Is a directory\n"
    );

    let out = common::cmd(dir.path())
        .current_dir(dir.path())
        .args(["-", "no/dir/out"])
        .write_stdin("x\n")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "runiq: no/dir/out: No such file or directory\n"
    );
}
