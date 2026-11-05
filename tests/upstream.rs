//! The original uniq test suite (tests/upstream/uniq_test.sh, vendored
//! verbatim from Apple text_cmds-199), run through a small ATF shim.
//!
//! Each test case runs in its own temporary directory and process group (the
//! "interactive" cases leave `yes | uniq` pipelines behind, which are killed
//! afterwards), with a `uniq` symlink first on PATH.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn upstream(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/upstream")
        .join(file)
}

/// Test case names, from `atf_init_test_cases`.
fn case_names() -> Vec<String> {
    let script = std::fs::read_to_string(upstream("uniq_test.sh")).unwrap();
    script
        .lines()
        .filter_map(|l| l.trim().strip_prefix("atf_add_test_case "))
        .map(|n| n.trim().to_string())
        .collect()
}

/// Run every case with `binary` installed as `uniq`; return the failures.
/// `atf_env` sets the variable Apple's ATF runner sets (the original only
/// line-buffers its output under it).
fn run_suite(binary: &Path, atf_env: bool) -> Vec<String> {
    let bin = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(binary, bin.path().join("uniq")).unwrap();
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let names = case_names();
    assert_eq!(names.len(), 15, "upstream suite changed: {names:?}");
    let mut failures = Vec::new();
    for name in names {
        let work = tempfile::tempdir().unwrap();
        let script = format!(
            ". '{}'; . '{}'; {name}_body",
            upstream("atf_shim.sh").display(),
            upstream("uniq_test.sh").display()
        );
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg(script)
            .current_dir(work.path())
            .env("PATH", &path)
            .env_remove("POSIXLY_CORRECT")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        if atf_env {
            cmd.env("__RUNNING_INSIDE_ATF_RUN", "1");
        } else {
            cmd.env_remove("__RUNNING_INSIDE_ATF_RUN");
        }
        let mut child = cmd.spawn().unwrap();
        let pgid = child.id() as i32;
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() > deadline {
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        // SAFETY: kill(2) on our own process group; reaps leftover pipelines.
        unsafe { libc::killpg(pgid, libc::SIGKILL) };
        let out = child.wait_with_output().unwrap();
        if status.is_none_or(|s| !s.success()) {
            failures.push(format!(
                "{name}: {status:?}\n{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    failures
}

#[test]
fn upstream_suite_passes_for_uniqr() {
    let failures = run_suite(Path::new(env!("CARGO_BIN_EXE_uniqr")), false);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The shim is only trustworthy if the original passes through it too.
#[cfg(target_os = "macos")]
#[test]
fn upstream_suite_passes_for_the_original() {
    let failures = run_suite(Path::new("/usr/bin/uniq"), true);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
