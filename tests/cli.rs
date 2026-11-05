//! Behaviour of --version and --help, and global conventions (exit codes,
//! stdout/stderr separation).

mod common;

use common::cmd;

#[test]
fn version_goes_to_stdout() {
    let home = tempfile::tempdir().unwrap();
    cmd(home.path())
        .arg("--version")
        .assert()
        .success()
        .stdout(format!(
            "{} {}\n",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ))
        .stderr("");
}

#[test]
fn usage_errors_exit_1_with_the_usage_line() {
    let home = tempfile::tempdir().unwrap();
    let output = cmd(home.path())
        .arg("--definitely-not-a-flag")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("unrecognized option `--definitely-not-a-flag'"),
        "{stderr}"
    );
    assert!(stderr.contains("\nusage: "), "{stderr}");
}

#[test]
fn only_help_and_version_extend_the_original() {
    // Anything else the original rejects is rejected here too.
    let home = tempfile::tempdir().unwrap();
    for args in [
        &["--generate", "man"][..],
        &["--color"],
        &["--json"],
        &["--vers"],
    ] {
        cmd(home.path()).args(args).assert().code(1);
    }
}
