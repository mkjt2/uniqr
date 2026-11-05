//! Errors and how they are shown. Every error exits 1, as in the original.

/// The exit status of every error.
pub const EXIT_CODE: i32 = 1;

#[derive(Debug)]
pub enum AppError {
    /// A BSD-style usage error: an optional getopt diagnostic, then the usage line.
    BsdUsage(Option<String>),
    Io(std::io::Error),
    Other(String),
}

/// The strerror(3) text of an I/O error, without Rust's ` (os error N)` suffix.
pub fn errno_message(e: &std::io::Error) -> String {
    let text = e.to_string();
    match (e.raw_os_error(), text.rfind(" (os error ")) {
        (Some(_), Some(i)) => text[..i].to_string(),
        _ => text,
    }
}

/// The program name for messages: the basename of argv\[0\], as BSD
/// getprogname(3) reports it (a symlink named `uniq` prints `uniq: ...`).
static PROGNAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Record argv\[0\]. Later calls are ignored.
pub fn set_progname(argv0: &std::ffi::OsStr) {
    let path = std::path::Path::new(argv0);
    if let Some(name) = path.file_name() {
        let _ = PROGNAME.set(name.to_string_lossy().into_owned());
    }
}

/// The recorded program name, or the CLI's name before argv\[0\] is known.
pub fn progname() -> &'static str {
    PROGNAME.get().map_or(crate::NAME, String::as_str)
}

/// `<name>: <message>`. A BSD usage error is its diagnostic (if any) followed
/// by the usage line.
pub fn render(err: &AppError, name: &str) -> String {
    match err {
        AppError::BsdUsage(message) => {
            let usage = format!(
                "usage: {name} [-cdiu] [-D[septype]] [-f fields] [-s chars] [input [output]]"
            );
            match message {
                Some(m) => format!("{name}: {m}\n{usage}"),
                None => usage,
            }
        }
        AppError::Io(e) => format!("{name}: I/O error: {e}"),
        AppError::Other(m) => format!("{name}: {m}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_prefixed_with_the_name() {
        let other = AppError::Other("cannot read: x".into());
        assert_eq!(render(&other, "tool"), "tool: cannot read: x");
        let io = AppError::Io(std::io::Error::other("boom"));
        assert_eq!(render(&io, "tool"), "tool: I/O error: boom");
    }

    #[test]
    fn usage_errors_end_with_the_usage_line() {
        let usage = "usage: tool [-cdiu] [-D[septype]] [-f fields] [-s chars] [input [output]]";
        assert_eq!(render(&AppError::BsdUsage(None), "tool"), usage);
        assert_eq!(
            render(&AppError::BsdUsage(Some("bad".into())), "tool"),
            format!("tool: bad\n{usage}")
        );
    }

    #[test]
    fn errno_messages_drop_the_os_error_suffix() {
        let e = std::io::Error::from_raw_os_error(libc::ENOENT);
        assert_eq!(errno_message(&e), "No such file or directory");
    }
}
