//! BSD argument parsing, emulated exactly.
//!
//! macOS uniq parses its arguments with an obsolete-form rewrite followed by
//! the FreeBSD-derived `getopt_long(3)` in libc (optstring `"+D::cdif:s:u"`).
//! clap cannot express that grammar (options stop at the first operand, `-D`
//! takes a value only when attached, a `-` inside a cluster starts a long
//! option, usage errors exit 1 with getopt's wording), so the arguments are
//! parsed here first and handed to clap in a canonical form it always accepts.

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};

/// Why the arguments were rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum ArgError {
    /// usage(): an optional getopt diagnostic, then the usage line.
    Usage(Option<String>),
    /// errx(1, ...) without the usage line (bad -f/-s values).
    Fatal(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HasArg {
    No,
    Required,
    Optional,
}

/// `long_opts` in uniq.c, in its order (the order matters for abbreviation
/// ambiguity), plus runiq's --help and --version.
const LONG: &[(&str, HasArg, u8)] = &[
    ("all-repeated", HasArg::Optional, b'D'),
    ("count", HasArg::No, b'c'),
    ("repeated", HasArg::No, b'd'),
    ("skip-fields", HasArg::Required, b'f'),
    ("ignore-case", HasArg::No, b'i'),
    ("skip-chars", HasArg::Required, b's'),
    ("unique", HasArg::No, b'u'),
    ("help", HasArg::No, b'H'),
    ("version", HasArg::No, b'V'),
];

const SHORT: &[(u8, HasArg)] = &[
    (b'D', HasArg::Optional),
    (b'c', HasArg::No),
    (b'd', HasArg::No),
    (b'i', HasArg::No),
    (b'f', HasArg::Required),
    (b's', HasArg::Required),
    (b'u', HasArg::No),
];

#[derive(Default)]
struct Opts {
    count: bool,
    repeated: bool,
    ignore_case: bool,
    unique: bool,
    all: Option<&'static str>,
    fields: Option<i64>,
    chars: Option<i64>,
}

/// Parse `args` (argv\[0\] first) the way macOS uniq does and return the
/// equivalent canonical argument list for clap.
pub fn normalize(args: &[OsString], posixly_correct: bool) -> Result<Vec<OsString>, ArgError> {
    let mut argv: Vec<Vec<u8>> = args.iter().map(|a| a.as_bytes().to_vec()).collect();
    obsolete(&mut argv);

    let mut opts = Opts::default();
    let mut i = 1;
    'args: while i < argv.len() {
        let arg = argv[i].clone();
        if arg == b"--" {
            i += 1;
            break;
        }
        if arg.first() != Some(&b'-') || arg.len() == 1 {
            break;
        }
        let mut p = 1;
        loop {
            if arg[p] == b'-' {
                // `--name` at the start of a word, or `-c-name` inside a cluster.
                let name = &arg[p + 1..];
                if name.is_empty() {
                    return Err(ArgError::Usage(None));
                }
                i += 1;
                let (opt, value) = long_option(name, &argv, &mut i)?;
                if let Some(canonical) = apply(&mut opts, opt, value)? {
                    return Ok(vec![args[0].clone(), canonical.into()]);
                }
                continue 'args;
            }
            let c = arg[p];
            p += 1;
            let Some(&(_, has_arg)) = SHORT.iter().find(|(s, _)| *s == c) else {
                let word = if posixly_correct {
                    "illegal"
                } else {
                    "invalid"
                };
                return Err(ArgError::Usage(Some(format!(
                    "{word} option -- {}",
                    char::from(c)
                ))));
            };
            match has_arg {
                HasArg::No => {
                    apply(&mut opts, c, None)?;
                    if p == arg.len() {
                        i += 1;
                        continue 'args;
                    }
                }
                HasArg::Optional => {
                    let value = (p < arg.len()).then(|| arg[p..].to_vec());
                    apply(&mut opts, c, value)?;
                    i += 1;
                    continue 'args;
                }
                HasArg::Required => {
                    let value = if p < arg.len() {
                        arg[p..].to_vec()
                    } else {
                        i += 1;
                        match argv.get(i) {
                            Some(v) => v.clone(),
                            None => {
                                return Err(ArgError::Usage(Some(format!(
                                    "option requires an argument -- {}",
                                    char::from(c)
                                ))));
                            }
                        }
                    };
                    apply(&mut opts, c, Some(value))?;
                    i += 1;
                    continue 'args;
                }
            }
        }
    }

    let operands = &argv[i..];
    if operands.len() > 2 {
        return Err(ArgError::Usage(None));
    }
    let mut out = vec![args[0].clone()];
    let flags = [
        (opts.count, "--count"),
        (opts.repeated, "--repeated"),
        (opts.ignore_case, "--ignore-case"),
        (opts.unique, "--unique"),
    ];
    out.extend(flags.iter().filter(|f| f.0).map(|f| OsString::from(f.1)));
    if let Some(all) = opts.all {
        out.push(format!("--all-repeated={all}").into());
    }
    if let Some(n) = opts.fields {
        out.push(format!("--skip-fields={n}").into());
    }
    if let Some(n) = opts.chars {
        out.push(format!("--skip-chars={n}").into());
    }
    out.push("--".into());
    out.extend(operands.iter().map(|o| OsString::from_vec(o.clone())));
    Ok(out)
}

/// `obsolete()` in uniq.c: `-NUM` becomes `-fNUM` and `+NUM` becomes `-sNUM`,
/// for every word up to the first one that is neither (nor `--`).
fn obsolete(argv: &mut [Vec<u8>]) {
    for ap in argv.iter_mut().skip(1) {
        match ap.first() {
            Some(b'-') if ap.get(1) == Some(&b'-') => return,
            Some(b'-') | Some(b'+') => {}
            _ => return,
        }
        if ap.get(1).is_some_and(u8::is_ascii_digit) {
            let letter = if ap[0] == b'+' { b's' } else { b'f' };
            let mut word = vec![b'-', letter];
            word.extend_from_slice(&ap[1..]);
            *ap = word;
        }
    }
}

/// `parse_long_options()` from FreeBSD getopt_long.c. `name` is the text after
/// the dashes; `i` already points past the current word.
fn long_option(
    name: &[u8],
    argv: &[Vec<u8>],
    i: &mut usize,
) -> Result<(u8, Option<Vec<u8>>), ArgError> {
    let lossy = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let (typed, value) = match memchr::memchr(b'=', name) {
        Some(eq) => (&name[..eq], Some(name[eq + 1..].to_vec())),
        None => (name, None),
    };
    let mut found: Option<usize> = None;
    for (k, (long, has_arg, val)) in LONG.iter().enumerate() {
        if !long.as_bytes().starts_with(typed) {
            continue;
        }
        if long.len() == typed.len() {
            found = Some(k);
            break;
        }
        if matches!(*val, b'H' | b'V') {
            // runiq's extensions match only when spelled out, so every
            // abbreviation behaves exactly as in the original.
            continue;
        }
        match found {
            None => found = Some(k),
            Some(m) if LONG[m].1 != *has_arg || LONG[m].2 != *val => {
                return Err(ArgError::Usage(Some(format!(
                    "option `--{}' is ambiguous",
                    lossy(typed)
                ))));
            }
            Some(_) => {}
        }
    }
    let Some(k) = found else {
        return Err(ArgError::Usage(Some(format!(
            "unrecognized option `--{}'",
            lossy(name)
        ))));
    };
    let (_, has_arg, val) = LONG[k];
    match (has_arg, value) {
        (HasArg::No, Some(_)) => Err(ArgError::Usage(Some(format!(
            "option `--{}' doesn't allow an argument",
            lossy(typed)
        )))),
        (HasArg::Required, None) => match argv.get(*i) {
            Some(v) => {
                *i += 1;
                Ok((val, Some(v.clone())))
            }
            None => Err(ArgError::Usage(Some(format!(
                "option `--{}' requires an argument",
                lossy(name)
            )))),
        },
        (_, value) => Ok((val, value)),
    }
}

/// The `switch` in uniq.c's option loop. Returns a canonical argument when the
/// option ends parsing (--help, --version).
fn apply(
    opts: &mut Opts,
    opt: u8,
    value: Option<Vec<u8>>,
) -> Result<Option<&'static str>, ArgError> {
    match opt {
        b'c' => opts.count = true,
        b'd' => opts.repeated = true,
        b'i' => opts.ignore_case = true,
        b'u' => opts.unique = true,
        b'D' => {
            let septype = value.as_deref().unwrap_or(b"none");
            opts.all = Some(
                ["none", "prepend", "separate"]
                    .into_iter()
                    .find(|s| s.as_bytes().eq_ignore_ascii_case(septype))
                    .ok_or(ArgError::Usage(None))?,
            );
        }
        b'f' => opts.fields = Some(skip_value("field", value.as_deref().unwrap_or_default())?),
        b's' => {
            opts.chars = Some(skip_value(
                "character",
                value.as_deref().unwrap_or_default(),
            )?)
        }
        b'H' => return Ok(Some("--help")),
        _ => return Ok(Some("--version")),
    }
    Ok(None)
}

/// `strtonum(value, 0, INT_MAX)` with uniq's error messages.
fn skip_value(what: &str, value: &[u8]) -> Result<i64, ArgError> {
    let fail = |why: &str| {
        ArgError::Fatal(format!(
            "{what} skip value is {why}: {}",
            String::from_utf8_lossy(value)
        ))
    };
    match strtoll(value) {
        None => Err(fail("invalid")),
        Some(n) if n < 0 => Err(fail("too small")),
        Some(n) if n > i64::from(i32::MAX) => Err(fail("too large")),
        Some(n) => Ok(n),
    }
}

/// strtoll(3) in base 10 that must consume the whole string: leading white
/// space and a sign are allowed; out-of-range values saturate like ERANGE.
fn strtoll(s: &[u8]) -> Option<i64> {
    let s = &s[s.iter().position(|b| !b" \t\n\x0b\x0c\r".contains(b))?..];
    let (negative, digits) = match s.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, s),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut n: i64 = 0;
    for d in digits {
        let d = i64::from(d - b'0');
        n = match n.checked_mul(10).and_then(|n| {
            if negative {
                n.checked_sub(d)
            } else {
                n.checked_add(d)
            }
        }) {
            Some(n) => n,
            None => return Some(if negative { i64::MIN } else { i64::MAX }),
        };
    }
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(args: &[&str]) -> Result<Vec<String>, ArgError> {
        let args: Vec<OsString> = std::iter::once("uniq")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        normalize(&args, false).map(|v| {
            v.into_iter()
                .skip(1)
                .map(|a| a.into_string().unwrap())
                .collect()
        })
    }

    #[test]
    fn canonical_form() {
        assert_eq!(
            norm(&["-cdiu", "-DSeparate", "-f", " 2", "-s3", "in", "out"]).unwrap(),
            [
                "--count",
                "--repeated",
                "--ignore-case",
                "--unique",
                "--all-repeated=separate",
                "--skip-fields=2",
                "--skip-chars=3",
                "--",
                "in",
                "out"
            ]
        );
        assert_eq!(norm(&[]).unwrap(), ["--"]);
        assert_eq!(norm(&["-"]).unwrap(), ["--", "-"]);
        assert_eq!(norm(&["--help", "-x"]).unwrap(), ["--help"]);
    }

    #[test]
    fn strtoll_edges() {
        assert_eq!(strtoll(b"  -0"), Some(0));
        assert_eq!(strtoll(b"\t+12"), Some(12));
        assert_eq!(strtoll(b"9223372036854775807"), Some(i64::MAX));
        assert_eq!(strtoll(b"9223372036854775808"), Some(i64::MAX));
        assert_eq!(strtoll(b"-9223372036854775809"), Some(i64::MIN));
        assert_eq!(strtoll(b"   "), None);
        assert_eq!(strtoll(b"+"), None);
        assert_eq!(strtoll(b"1 "), None);
    }

    #[test]
    fn non_utf8_arguments_survive() {
        let raw = OsString::from_vec(b"in\xff".to_vec());
        let out = normalize(&["uniq".into(), raw.clone()], false).unwrap();
        assert_eq!(out.last(), Some(&raw));
    }
}
