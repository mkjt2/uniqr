//! Increment 3: locale-aware comparison keys for -f, -s and -i.
//!
//! The table in `data/locale_cases.rs` was recorded from macOS /usr/bin/uniq by
//! `scripts/record_locale_cases.py` (byte-exact, including invalid UTF-8 and
//! legacy encodings that cli.toml cases cannot hold). Cases marked
//! non-portable depend on macOS libc tables or locales and only run there.

mod common;
#[path = "data/locale_cases.rs"]
mod locale_cases;

fn render(b: &[u8]) -> String {
    String::from_utf8_lossy(b).escape_debug().to_string()
}

#[test]
fn recorded_locale_cases() {
    let mut failures = Vec::new();
    for &(name, locale, args, stdin, want, code, portable) in locale_cases::CASES {
        if !portable && !cfg!(target_os = "macos") {
            continue;
        }
        let dir = tempfile::tempdir().unwrap();
        let out = common::cmd(dir.path())
            .env_remove("LANG")
            .env_remove("LC_CTYPE")
            .env_remove("LC_COLLATE")
            .env("LC_ALL", locale)
            .args(args)
            .write_stdin(stdin.to_vec())
            .output()
            .unwrap();
        if out.stdout != want || out.status.code() != Some(code) {
            failures.push(format!(
                "{name} (LC_ALL={locale} {args:?}): got {:?} \"{}\", want {code} \"{}\"",
                out.status.code(),
                render(&out.stdout),
                render(want)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn locale_categories_resolve_like_setlocale() {
    let run = |env: &[(&str, &str)]| {
        let dir = tempfile::tempdir().unwrap();
        let mut c = common::cmd(dir.path());
        for k in ["LC_ALL", "LC_CTYPE", "LC_COLLATE", "LANG"] {
            c.env_remove(k);
        }
        for (k, v) in env {
            c.env(k, v);
        }
        let out = c.arg("-ic").write_stdin("É\né\n").output().unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let folded = "   2 É\n";
    let unfolded = "   1 É\n   1 é\n";
    assert_eq!(run(&[("LANG", "en_US.UTF-8")]), folded);
    assert_eq!(run(&[("LANG", "C"), ("LC_CTYPE", "en_US.UTF-8")]), folded);
    assert_eq!(run(&[("LANG", "en_US.UTF-8"), ("LC_ALL", "C")]), unfolded);
    // With no locale variables, setlocale picks C; musl's default is C.UTF-8.
    let default = if cfg!(target_env = "musl") {
        folded
    } else {
        unfolded
    };
    assert_eq!(run(&[]), default);
}

/// Collation follows LC_COLLATE, decoding follows LC_CTYPE (macOS values).
#[cfg(target_os = "macos")]
#[test]
fn collation_and_ctype_are_separate_categories() {
    let run = |env: &[(&str, &str)]| {
        let dir = tempfile::tempdir().unwrap();
        let mut c = common::cmd(dir.path());
        for k in ["LC_ALL", "LC_CTYPE", "LC_COLLATE", "LANG", "PATH_LOCALE"] {
            c.env_remove(k);
        }
        for (k, v) in env {
            c.env(k, v);
        }
        let out = c.arg("-c").write_stdin("ж\nя\n").output().unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let merged = "   2 ж\n";
    let apart = "   1 ж\n   1 я\n";
    assert_eq!(run(&[("LANG", "en_US.UTF-8")]), merged);
    assert_eq!(run(&[("LANG", "en_US.UTF-8"), ("LC_COLLATE", "C")]), apart);
    assert_eq!(
        run(&[("LC_CTYPE", "C"), ("LC_COLLATE", "en_US.UTF-8")]),
        apart
    );
    // macOS libc ignores $PATH_LOCALE (so does the original).
    let empty = tempfile::tempdir().unwrap();
    let path = empty.path().to_str().unwrap();
    assert_eq!(
        run(&[("LANG", "en_US.UTF-8"), ("PATH_LOCALE", path)]),
        merged
    );
}

#[test]
fn ascii_options_without_locale() {
    let run = |args: &[&str], stdin: &str| {
        let dir = tempfile::tempdir().unwrap();
        let out = common::cmd(dir.path())
            .env("LC_ALL", "C")
            .args(args)
            .write_stdin(stdin)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(run(&["-i"], "a\nA\nb\nB\na\nA\n"), "a\nb\na\n");
    assert_eq!(run(&["-f", "1"], "1\ta\n2\ta\n3\tb\n"), "1\ta\n3\tb\n");
    assert_eq!(
        run(&["-f", "2", "-c"], "  a b x\n c\td x\n"),
        "   2   a b x\n"
    );
    assert_eq!(run(&["-f", "1", "-c"], "   \n \t\n"), "   2    \n");
    assert_eq!(run(&["-s", "2"], "1 a\n2 a\n3 b\n"), "1 a\n3 b\n");
    assert_eq!(run(&["-s", "9", "-c"], "abc\nxyz\n"), "   2 abc\n");
    assert_eq!(
        run(&["-f", "0", "-s", "0", "-c"], "a\nb\n"),
        "   1 a\n   1 b\n"
    );
    assert_eq!(run(&["-ic"], "A\0x\na\0y\n"), "   2 A");
    assert_eq!(run(&["-ic"], "Ab\naB"), "   2 Ab\n");
    assert_eq!(
        run(&["-ic", "-f1"], "x AB\ny ab\nz abc\n"),
        "   2 x AB\n   1 z abc\n"
    );
}

/// Plain mode defers setlocale until it meets a non-ASCII mismatch, and only
/// when the locale names make that safe. These pin the edge cases of that
/// laziness against the original (macOS values).
#[cfg(target_os = "macos")]
#[test]
fn lazy_locale_setup_in_plain_mode() {
    let run = |env: &[(&str, &str)], stdin: &[u8]| {
        let dir = tempfile::tempdir().unwrap();
        let mut c = common::cmd(dir.path());
        for k in ["LC_ALL", "LC_CTYPE", "LC_COLLATE", "LANG"] {
            c.env_remove(k);
        }
        for (k, v) in env {
            c.env(k, v);
        }
        c.arg("-c")
            .write_stdin(stdin.to_vec())
            .output()
            .unwrap()
            .stdout
    };
    // ASCII first, then a collation-equal non-ASCII pair: setup happens late.
    assert_eq!(
        run(&[("LANG", "en_US.UTF-8")], "a\nb\nж\nя\n".as_bytes()),
        "   1 a\n   1 b\n   2 ж\n".as_bytes()
    );
    // Empty LC_ALL is ignored; LC_COLLATE from a non-UTF-8 locale disables
    // the shortcut (PT154 ignores \x01 even in ASCII lines).
    assert_eq!(
        run(&[("LC_ALL", ""), ("LANG", "kk_KZ.PT154")], b"a\x01b\nab\n"),
        b"   2 a\x01b\n"
    );
    assert_eq!(
        run(
            &[("LC_CTYPE", "en_US.UTF-8"), ("LC_COLLATE", "kk_KZ.PT154")],
            b"a\x01b\nab\n"
        ),
        b"   2 a\x01b\n"
    );
    // SJIS through LC_CTYPE alone also disables it.
    assert_eq!(
        run(
            &[("LC_CTYPE", "ja_JP.SJIS"), ("LC_COLLATE", "ja_JP.SJIS")],
            b"a\\b\nab\n"
        ),
        b"   2 a\\b\n"
    );
    // A UTF-8 name that does not exist falls back to C: bytes.
    assert_eq!(
        run(&[("LANG", "xx_YY.UTF-8")], "ж\nя\n".as_bytes()),
        "   1 ж\n   1 я\n".as_bytes()
    );
}

/// LC_COLLATE, not LC_CTYPE, decides whether ASCII can collate equal: the
/// key path (-f/-s/-i) must ask wcscoll here (macOS values).
#[cfg(target_os = "macos")]
#[test]
fn utf8_ctype_with_single_byte_collation() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::cmd(dir.path());
    for k in ["LC_ALL", "LANG"] {
        c.env_remove(k);
    }
    let out = c
        .env("LC_CTYPE", "en_US.UTF-8")
        .env("LC_COLLATE", "kk_KZ.PT154")
        .args(["-f1", "-c"])
        .write_stdin("1 a\u{1}b\n2 ab\n")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "   2 1 a\u{1}b\n");
}
