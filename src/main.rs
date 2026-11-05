//! Process entry point: wires real stdio into `uniqr::run`.
//!
//! `no_main` keeps the Rust runtime's startup out of the way. That startup
//! sets SIGPIPE to "ignore"; macOS uniq inherits whatever disposition its
//! parent gave it, so with the default it dies quietly on a closed pipe
//! (`uniqr big | head`), and with SIGPIPE ignored it reports
//! `stdout: Broken pipe` and exits 1. argv comes straight from C `main`.
// The test build keeps libtest's own main.
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
#[unsafe(no_mangle)]
extern "C" fn main(argc: std::ffi::c_int, argv: *const *const std::ffi::c_char) -> std::ffi::c_int {
    use std::ffi::{CStr, OsString};
    use std::fs::File;
    use std::mem::ManuallyDrop;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStringExt;

    // SAFETY: the C runtime passes `argc` valid NUL-terminated strings.
    let args: Vec<OsString> = (0..usize::try_from(argc).unwrap_or(0))
        .map(|i| unsafe { CStr::from_ptr(*argv.add(i)) })
        .map(|s| OsString::from_vec(s.to_bytes().to_vec()))
        .collect();

    // fd 1 directly: the engine buffers its own output, and std's stdout is a
    // LineWriter that only the (skipped) runtime would flush at exit.
    // SAFETY: fd 1 stays open for the life of the process; ManuallyDrop
    // keeps it from being closed here.
    let mut stdout = ManuallyDrop::new(unsafe { File::from_raw_fd(1) });
    let mut out = std::io::BufWriter::new(&mut *stdout);
    let mut err = std::io::stderr().lock();
    let mut input = std::io::stdin().lock();
    let env = |key: &str| std::env::var(key).ok();
    let code = uniqr::run(
        args,
        uniqr::Io {
            out: &mut out,
            err: &mut err,
            input: &mut input,
            env: &env,
        },
    );
    uniqr::flush_stdout(&mut out, &mut err, code)
}
