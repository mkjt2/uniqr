//! Increment 4: process-level behavior. SIGPIPE disposition is inherited,
//! write errors are reported like the original, and output is flushed
//! before blocking on input.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::Duration;

fn runiq() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_runiq"));
    c.env("LC_ALL", "C");
    c
}

/// Many distinct lines, so output keeps flowing after the reader is gone.
fn many_lines() -> Vec<u8> {
    (0..400_000)
        .flat_map(|i| format!("{i}\n").into_bytes())
        .collect()
}

#[test]
fn closed_pipe_kills_quietly_by_default() {
    // std::process::Command restores SIGPIPE to its default in the child.
    let mut child = runiq()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&many_lines());
    });
    let out = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert_eq!(out.status.signal(), Some(libc::SIGPIPE), "{:?}", out.status);
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn closed_pipe_with_sigpipe_ignored_is_an_error() {
    // An ignored SIGPIPE is inherited through exec: writes fail with EPIPE.
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(format!(
            "trap '' PIPE; '{}' <\"$0\" 2>err | true; cat err >&2",
            env!("CARGO_BIN_EXE_runiq")
        ))
        .arg(write_temp(&many_lines()))
        .current_dir(dir.path())
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "runiq: stdout: Broken pipe\n"
    );
}

#[test]
fn write_error_on_output_file_names_the_file() {
    let script = format!(
        "trap '' PIPE; mkfifo f; (head -c 1 f >/dev/null) & '{}' \"$0\" f 2>err; echo $? >code; cat err",
        env!("CARGO_BIN_EXE_runiq")
    );
    let dir = tempfile::tempdir().unwrap();
    let input = write_temp(&many_lines());
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg(&input)
        .current_dir(dir.path())
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    let code = std::fs::read_to_string(dir.path().join("code")).unwrap();
    assert_eq!(code.trim(), "1");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "runiq: f: Broken pipe\n"
    );
}

#[test]
fn output_is_flushed_before_waiting_for_input() {
    let mut child = runiq()
        .args(["-d"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    stdin.write_all(b"y\ny\ny\n").unwrap();
    stdin.flush().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        tx.send(line).unwrap();
        let mut rest = String::new();
        stdout.read_to_string(&mut rest).unwrap();
        rest
    });
    // The repeated line must arrive while stdin is still open.
    let line = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(line, "y\n");
    stdin.write_all(b"z\nz\n").unwrap();
    drop(stdin);
    assert_eq!(reader.join().unwrap(), "z\n");
    assert!(child.wait().unwrap().success());
}

fn write_temp(data: &[u8]) -> std::path::PathBuf {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), data).unwrap();
    file.into_temp_path().keep().unwrap()
}
