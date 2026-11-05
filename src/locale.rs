//! Locale-aware line equality.
//!
//! uniq.c converts each line with mbstowcs(3), skips fields (iswblank(3)) and
//! characters, lowercases with towlower(3) and calls lines equal when
//! wcscoll(3) returns 0. On macOS that is not byte equality: characters the
//! locale's LC_COLLATE does not define get no weight and are ignored (Apple's
//! wcscoll skips FreeBSD's final pass for UNDEFINED characters), so in
//! en_US.UTF-8 `ж` equals `я` and `aж` equals `a`. This module reproduces the
//! whole pipeline and only takes shortcuts that cannot change the answer:
//!
//! - identical keys are equal (wcscoll starts with wcscmp);
//! - with the C/POSIX collation (or on musl, whose wcscoll is wcscmp)
//!   different keys differ;
//! - in UTF-8 locales two different all-ASCII keys differ: every ASCII
//!   character has its own weights there. This holds for every UTF-8 locale
//!   macOS ships (checked by `tests/collation.rs`); it does not hold for some
//!   single-byte and legacy multibyte locales, which always ask wcscoll.
//!
//! Even setlocale is deferred while lines are ASCII: see [`Lazy`].
//!
//! Everything else goes to libc's wcscoll on wide strings built exactly as
//! uniq.c builds them. Lines that fail to decode are compared as raw bytes,
//! ignoring -f/-s/-i, when both fail, and differ when only one fails
//! (uniq.c's inlcmp fallback).

use crate::engine::Comparator;
use std::cell::{OnceCell, RefCell};
use std::ffi::{CStr, c_char, c_int};

#[allow(non_camel_case_types)]
#[cfg(target_os = "macos")]
type wint_t = i32;
#[allow(non_camel_case_types)]
#[cfg(not(target_os = "macos"))]
type wint_t = u32;
type Wide = libc::wchar_t;

unsafe extern "C" {
    fn btowc(c: c_int) -> wint_t;
    fn towlower(wc: wint_t) -> wint_t;
    fn iswblank(wc: wint_t) -> c_int;
    fn mbstowcs(dst: *mut Wide, src: *const c_char, n: usize) -> usize;
    fn wcscoll(a: *const Wide, b: *const Wide) -> c_int;
    #[cfg(target_os = "macos")]
    fn ___mb_cur_max() -> c_int;
    #[cfg(not(target_os = "macos"))]
    fn __ctype_get_mb_cur_max() -> usize;
}

const WEOF: u32 = u32::MAX;

fn lower(c: u32) -> u32 {
    // SAFETY: towlower is defined for every wint_t value.
    unsafe { towlower(c as wint_t) as u32 }
}

fn blank(c: u32) -> bool {
    // SAFETY: iswblank is defined for every wint_t value.
    unsafe { iswblank(c as wint_t) != 0 }
}

/// wcscoll(a, b) == 0 for NUL-terminated wide strings.
fn collate_equal(a: &[Wide], b: &[Wide]) -> bool {
    debug_assert!(a.last() == Some(&0) && b.last() == Some(&0));
    // SAFETY: both slices are NUL-terminated.
    unsafe { wcscoll(a.as_ptr(), b.as_ptr()) == 0 }
}

/// The current locale, as far as uniq is concerned.
pub struct Locale {
    pub charset: Charset,
    /// wcscoll can call different strings equal (a real LC_COLLATE table).
    pub collates: bool,
    /// Different all-ASCII wide strings never collate equal: LC_COLLATE is
    /// C/POSIX or a UTF-8 locale (see the module docs).
    pub ascii_distinct: bool,
}

/// LC_COLLATE names whose tables keep every ASCII string distinct.
fn ascii_distinct_collation(name: &str) -> bool {
    name == "C" || name == "POSIX" || name.to_ascii_lowercase().ends_with(".utf-8")
}

/// How the current locale encodes characters.
pub enum Charset {
    Utf8(Box<Ascii>),
    Single(Box<SingleByte>),
    Wide,
}

/// Classification of ASCII characters in the current locale.
pub struct Ascii {
    lower: [u32; 128],
    blank: [bool; 128],
    /// towlower keeps ASCII in ASCII (not so for `I` in Turkish locales).
    lower_stays_ascii: bool,
}

impl Ascii {
    fn build(lower_of: impl Fn(u8) -> u32, blank_of: impl Fn(u8) -> bool) -> Ascii {
        let mut a = Ascii {
            lower: [0; 128],
            blank: [false; 128],
            lower_stays_ascii: true,
        };
        for b in 0..128u8 {
            let l = lower_of(b);
            a.lower[usize::from(b)] = l;
            a.lower_stays_ascii &= l < 128;
            a.blank[usize::from(b)] = blank_of(b);
        }
        a
    }

    fn from_libc() -> Ascii {
        Ascii::build(|b| lower(u32::from(b)), |b| blank(u32::from(b)))
    }

    /// The C/POSIX classification, for tests.
    pub fn posix() -> Ascii {
        Ascii::build(
            |b| u32::from(b.to_ascii_lowercase()),
            |b| b == b' ' || b == b'\t',
        )
    }
}

/// Byte → character tables for a single-byte locale.
pub struct SingleByte {
    valid: [bool; 256],
    all_valid: bool,
    wide: [u32; 256],
    lower: [u32; 256],
    blank: [bool; 256],
}

impl SingleByte {
    fn from_libc() -> SingleByte {
        let mut t = SingleByte {
            valid: [false; 256],
            all_valid: true,
            wide: [0; 256],
            lower: [0; 256],
            blank: [false; 256],
        };
        for b in 1..256usize {
            // SAFETY: btowc accepts any int.
            let wc = unsafe { btowc(b as c_int) } as u32;
            if wc == WEOF {
                t.all_valid = false;
                continue;
            }
            t.valid[b] = true;
            t.wide[b] = wc;
            t.lower[b] = lower(wc);
            t.blank[b] = blank(wc);
        }
        t
    }
}

/// Apply the user's locale (setlocale(LC_ALL, "")) and classify it.
pub fn detect() -> Locale {
    // SAFETY: called once, before any other thread exists; the strings
    // returned by setlocale/nl_langinfo are copied before the locale changes.
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
        let collate_name = CStr::from_ptr(libc::setlocale(libc::LC_COLLATE, std::ptr::null()))
            .to_string_lossy()
            .into_owned();
        let collates = collation_table_loads(&collate_name);
        let ascii_distinct = ascii_distinct_collation(&collate_name);
        let codeset = CStr::from_ptr(libc::nl_langinfo(libc::CODESET));
        let charset = if codeset.to_bytes().eq_ignore_ascii_case(b"UTF-8") {
            Charset::Utf8(Box::new(Ascii::from_libc()))
        } else if mb_cur_max() == 1 {
            Charset::Single(Box::new(SingleByte::from_libc()))
        } else {
            Charset::Wide
        };
        Locale {
            charset,
            collates,
            ascii_distinct,
        }
    }
}

#[cfg(target_os = "macos")]
fn mb_cur_max() -> usize {
    // SAFETY: no preconditions.
    unsafe { ___mb_cur_max() as usize }
}

#[cfg(not(target_os = "macos"))]
fn mb_cur_max() -> usize {
    // SAFETY: no preconditions.
    unsafe { __ctype_get_mb_cur_max() }
}

/// Whether wcscoll can differ from wcscmp under the LC_COLLATE locale `name`.
/// C and POSIX use libc's built-in collation, which is wcscmp. Any other name
/// may load a table (if it has none, asking wcscoll is merely slower).
#[cfg(not(target_env = "musl"))]
fn collation_table_loads(name: &str) -> bool {
    name != "C" && name != "POSIX"
}

/// musl has no collation: wcscoll is wcscmp.
#[cfg(target_env = "musl")]
fn collation_table_loads(_: &str) -> bool {
    false
}

/// What to ignore and how to compare (-f, -s, -i).
#[derive(Debug, Clone, Copy, Default)]
pub struct KeySpec {
    pub fields: u64,
    pub chars: u64,
    pub fold: bool,
}

impl KeySpec {
    /// Whole lines, as they are.
    pub fn is_plain(&self) -> bool {
        self.fields == 0 && self.chars == 0 && !self.fold
    }
}

/// Per-line analysis: whether it decoded, and where its key starts.
#[derive(Debug, Clone, Copy, Default)]
pub struct Key {
    valid: bool,
    start: usize,
}

/// `skip()` from uniq.c over a character sequence of byte length `len`.
/// `at(i)` returns the character at byte offset `i` as (is_blank, length).
#[inline]
fn skip(len: usize, spec: &KeySpec, mut at: impl FnMut(usize) -> (bool, usize)) -> usize {
    let mut i = 0;
    let mut fields = 0;
    while i < len && fields != spec.fields {
        fields += 1;
        while i < len {
            let (is_blank, n) = at(i);
            if !is_blank {
                break;
            }
            i += n;
        }
        while i < len {
            let (is_blank, n) = at(i);
            if is_blank {
                break;
            }
            i += n;
        }
    }
    let mut chars = spec.chars;
    while chars > 0 && i < len {
        i += at(i).1;
        chars -= 1;
    }
    i
}

/// Decode the character at `i` of valid UTF-8.
#[inline]
fn utf8_at(s: &[u8], i: usize) -> (u32, usize) {
    let b = s[i];
    let cont = |k: usize| u32::from(s[i + k] & 0x3f);
    match b {
        0x00..=0x7f => (u32::from(b), 1),
        0xc0..=0xdf => ((u32::from(b & 0x1f) << 6) | cont(1), 2),
        0xe0..=0xef => ((u32::from(b & 0x0f) << 12) | (cont(1) << 6) | cont(2), 3),
        _ => (
            (u32::from(b & 0x07) << 18) | (cont(1) << 12) | (cont(2) << 6) | cont(3),
            4,
        ),
    }
}

fn utf8_valid(s: &[u8]) -> bool {
    s.is_ascii() || std::str::from_utf8(s).is_ok()
}

/// Scratch wide-string buffers for wcscoll.
type Scratch = RefCell<(Vec<Wide>, Vec<Wide>)>;

pub struct Utf8Keys {
    spec: KeySpec,
    ascii: Box<Ascii>,
    collates: bool,
    ascii_distinct: bool,
    scratch: Scratch,
}

impl Utf8Keys {
    /// `collates`/`ascii_distinct` as in [`Locale`].
    pub fn new(spec: KeySpec, ascii: Box<Ascii>, collates: bool, ascii_distinct: bool) -> Utf8Keys {
        Utf8Keys {
            spec,
            ascii,
            collates,
            ascii_distinct,
            scratch: RefCell::default(),
        }
    }

    #[inline]
    fn lower(&self, c: u32) -> u32 {
        if c < 128 {
            self.ascii.lower[c as usize]
        } else {
            lower(c)
        }
    }

    fn fold_eq(&self, a: &[u8], b: &[u8]) -> bool {
        if a.is_ascii() && b.is_ascii() {
            let l = &self.ascii.lower;
            return a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(x, y)| l[usize::from(*x)] == l[usize::from(*y)]);
        }
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            let (ca, na) = utf8_at(a, i);
            let (cb, nb) = utf8_at(b, j);
            if ca != cb && self.lower(ca) != self.lower(cb) {
                return false;
            }
            i += na;
            j += nb;
        }
        i == a.len() && j == b.len()
    }

    /// Different (after folding) valid keys: equal only if wcscoll says so.
    fn collate_eq(&self, a: &[u8], b: &[u8]) -> bool {
        let ascii_ok = self.ascii_distinct && (!self.spec.fold || self.ascii.lower_stays_ascii);
        if !self.collates || (ascii_ok && a.is_ascii() && b.is_ascii()) {
            return false;
        }
        let mut s = self.scratch.borrow_mut();
        let (wa, wb) = &mut *s;
        self.widen(a, wa);
        self.widen(b, wb);
        collate_equal(wa, wb)
    }

    fn widen(&self, s: &[u8], out: &mut Vec<Wide>) {
        out.clear();
        let mut i = 0;
        while i < s.len() {
            let (c, n) = utf8_at(s, i);
            let c = if self.spec.fold { self.lower(c) } else { c };
            out.push(c as Wide);
            i += n;
        }
        out.push(0);
    }
}

impl Comparator for Utf8Keys {
    type Info = Key;

    fn analyze(&self, line: &[u8]) -> Key {
        if self.spec.is_plain() {
            // Validity is only needed when lines differ; see `equal`.
            return Key::default();
        }
        if !utf8_valid(line) {
            return Key::default();
        }
        let ascii = &self.ascii;
        let start = skip(line.len(), &self.spec, |i| {
            let b = line[i];
            if b < 128 {
                (ascii.blank[usize::from(b)], 1)
            } else {
                let (c, n) = utf8_at(line, i);
                (blank(c), n)
            }
        });
        Key { valid: true, start }
    }

    fn equal(&self, a: &[u8], ai: Key, b: &[u8], bi: Key) -> bool {
        if self.spec.is_plain() {
            return a == b || (utf8_valid(a) && utf8_valid(b) && self.collate_eq(a, b));
        }
        match (ai.valid, bi.valid) {
            (true, true) => {
                let (a, b) = (&a[ai.start..], &b[bi.start..]);
                let same = if self.spec.fold {
                    self.fold_eq(a, b)
                } else {
                    a == b
                };
                same || self.collate_eq(a, b)
            }
            (false, false) => a == b,
            _ => false,
        }
    }
}

pub struct SingleByteKeys {
    spec: KeySpec,
    table: Box<SingleByte>,
    collates: bool,
    scratch: Scratch,
}

impl SingleByteKeys {
    pub fn new(spec: KeySpec, table: Box<SingleByte>, collates: bool) -> SingleByteKeys {
        SingleByteKeys {
            spec,
            table,
            collates,
            scratch: RefCell::default(),
        }
    }

    fn valid(&self, line: &[u8]) -> bool {
        let t = &self.table;
        t.all_valid || line.iter().all(|b| t.valid[usize::from(*b)])
    }

    fn widen(&self, s: &[u8], out: &mut Vec<Wide>) {
        let map = if self.spec.fold {
            &self.table.lower
        } else {
            &self.table.wide
        };
        out.clear();
        out.extend(s.iter().map(|b| map[usize::from(*b)] as Wide));
        out.push(0);
    }

    fn collate_eq(&self, a: &[u8], b: &[u8]) -> bool {
        if !self.collates {
            return false;
        }
        let mut s = self.scratch.borrow_mut();
        let (wa, wb) = &mut *s;
        self.widen(a, wa);
        self.widen(b, wb);
        collate_equal(wa, wb)
    }
}

impl Comparator for SingleByteKeys {
    type Info = Key;

    fn analyze(&self, line: &[u8]) -> Key {
        if !self.valid(line) {
            return Key::default();
        }
        let t = &self.table;
        let start = skip(line.len(), &self.spec, |i| {
            (t.blank[usize::from(line[i])], 1)
        });
        Key { valid: true, start }
    }

    fn equal(&self, a: &[u8], ai: Key, b: &[u8], bi: Key) -> bool {
        match (ai.valid, bi.valid) {
            (true, true) => {
                let (a, b) = (&a[ai.start..], &b[bi.start..]);
                let same = if self.spec.fold {
                    let l = &self.table.lower;
                    a.len() == b.len()
                        && a.iter()
                            .zip(b)
                            .all(|(x, y)| l[usize::from(*x)] == l[usize::from(*y)])
                } else {
                    a == b
                };
                same || self.collate_eq(a, b)
            }
            (false, false) => a == b,
            _ => false,
        }
    }
}

/// Any other multibyte locale: the original algorithm, verbatim.
pub struct WideKeys {
    spec: KeySpec,
    collates: bool,
    scratch: RefCell<(Vec<u8>, Vec<Wide>, Vec<Wide>)>,
}

impl WideKeys {
    pub fn new(spec: KeySpec, collates: bool) -> WideKeys {
        WideKeys {
            spec,
            collates,
            scratch: RefCell::default(),
        }
    }

    /// `convert()` from uniq.c into `out` (NUL-terminated); false when the
    /// line does not decode.
    fn convert(&self, line: &[u8], cstr: &mut Vec<u8>, out: &mut Vec<Wide>) -> bool {
        cstr.clear();
        cstr.extend_from_slice(line);
        cstr.push(0);
        // SAFETY: `cstr` is NUL-terminated (the engine cut the line at its
        // first NUL) and `out` has room for n + 1 wide characters.
        let n = unsafe { mbstowcs(std::ptr::null_mut(), cstr.as_ptr().cast(), 0) };
        if n == usize::MAX {
            return false;
        }
        out.clear();
        out.resize(n + 1, 0);
        // SAFETY: as above.
        unsafe { mbstowcs(out.as_mut_ptr(), cstr.as_ptr().cast(), n + 1) };
        out.truncate(n);
        let start = skip(out.len(), &self.spec, |i| (blank(out[i] as u32), 1));
        out.drain(..start);
        if self.spec.fold {
            for c in out.iter_mut() {
                *c = lower(*c as u32) as Wide;
            }
        }
        out.push(0);
        true
    }
}

impl Comparator for WideKeys {
    type Info = ();

    fn analyze(&self, _: &[u8]) {}

    fn equal(&self, a: &[u8], _: (), b: &[u8], _: ()) -> bool {
        if self.spec.is_plain() && a == b {
            return true;
        }
        let mut s = self.scratch.borrow_mut();
        let (cstr, wa, wb) = &mut *s;
        let va = self.convert(a, cstr, wa);
        let vb = self.convert(b, cstr, wb);
        match (va, vb) {
            (true, true) => wa == wb || (self.collates && collate_equal(wa, wb)),
            (false, false) => a == b,
            _ => false,
        }
    }
}

/// The locale-backed comparator a [`Lazy`] switches to once it needs one.
enum Full {
    Utf8(Utf8Keys),
    Single(SingleByteKeys),
    Wide(WideKeys),
}

impl Full {
    fn analyze(&self, line: &[u8]) -> Key {
        match self {
            Full::Utf8(k) => k.analyze(line),
            Full::Single(k) => k.analyze(line),
            Full::Wide(_) => Key::default(),
        }
    }

    fn equal(&self, a: &[u8], ai: Key, b: &[u8], bi: Key) -> bool {
        match self {
            Full::Utf8(k) => k.equal(a, ai, b, bi),
            Full::Single(k) => k.equal(a, ai, b, bi),
            Full::Wide(k) => k.equal(a, (), b, ()),
        }
    }

    /// Equality of two lines analyzed from scratch.
    fn line_eq(&self, a: &[u8], b: &[u8]) -> bool {
        let (ai, bi) = (self.analyze(a), self.analyze(b));
        self.equal(a, ai, b, bi)
    }
}

/// Comparison with locale setup deferred until a non-ASCII line needs it.
///
/// setlocale and its tables cost the process ~140 KB, and for ASCII lines
/// they cannot change the answer when the LC_CTYPE and LC_COLLATE names
/// resolve (LC_ALL, then the category, then LANG, ignoring empty values) to
/// C, POSIX or a `*.UTF-8` locale. Either such a locale exists, and then
/// ASCII characters fold, count as blanks and collate exactly as in C
/// (re-verified for every shipped locale by `tests/collation.rs`), or it does
/// not, and setlocale falls back to C. So ASCII keys are handled here with C
/// rules; the first non-ASCII line sets the locale up.
pub struct Lazy {
    spec: KeySpec,
    ascii: Ascii,
    full: OnceCell<Full>,
}

impl Lazy {
    /// `None` when the locale names do not allow deferring setlocale.
    pub fn new(spec: KeySpec, env: &dyn Fn(&str) -> Option<String>) -> Option<Lazy> {
        let resolve = |category: &str| {
            ["LC_ALL", category, "LANG"]
                .iter()
                .find_map(|k| env(k).filter(|v| !v.is_empty()))
                .unwrap_or_else(|| "C".into())
        };
        let safe = ascii_distinct_collation(&resolve("LC_CTYPE"))
            && ascii_distinct_collation(&resolve("LC_COLLATE"));
        safe.then(|| Lazy {
            spec,
            ascii: Ascii::posix(),
            full: OnceCell::new(),
        })
    }

    fn full(&self) -> &Full {
        self.full.get_or_init(|| {
            let loc = detect();
            let spec = self.spec;
            match loc.charset {
                Charset::Utf8(a) => {
                    Full::Utf8(Utf8Keys::new(spec, a, loc.collates, loc.ascii_distinct))
                }
                Charset::Single(t) => Full::Single(SingleByteKeys::new(spec, t, loc.collates)),
                Charset::Wide => Full::Wide(WideKeys::new(spec, loc.collates)),
            }
        })
    }
}

impl Comparator for Lazy {
    type Info = Key;

    #[inline]
    fn analyze(&self, line: &[u8]) -> Key {
        if self.spec.is_plain() {
            return Key::default();
        }
        if !line.is_ascii() {
            return self.full().analyze(line);
        }
        let blank = &self.ascii.blank;
        let start = skip(line.len(), &self.spec, |i| (blank[usize::from(line[i])], 1));
        Key { valid: true, start }
    }

    #[inline]
    fn equal(&self, a: &[u8], ai: Key, b: &[u8], bi: Key) -> bool {
        let ascii = a.is_ascii() && b.is_ascii();
        if self.spec.is_plain() {
            return a == b || (!ascii && self.full().line_eq(a, b));
        }
        if !ascii {
            return self.full().equal(a, ai, b, bi);
        }
        let (a, b) = (&a[ai.start..], &b[bi.start..]);
        if self.spec.fold {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELDS1: KeySpec = KeySpec {
        fields: 1,
        chars: 0,
        fold: false,
    };

    #[test]
    fn skip_follows_uniq_c() {
        let ascii = Ascii::posix();
        let skip_ascii =
            |s: &[u8], spec: KeySpec| skip(s.len(), &spec, |i| (ascii.blank[usize::from(s[i])], 1));
        assert_eq!(skip_ascii(b" \ta  b", FIELDS1), 3);
        let spec = KeySpec {
            fields: 2,
            chars: 1,
            fold: false,
        };
        assert_eq!(skip_ascii(b"a b cd", spec), 4);
        assert_eq!(skip_ascii(b"a", spec), 1);
        let fold_only = KeySpec {
            fold: true,
            ..KeySpec::default()
        };
        assert_eq!(skip_ascii(b"  x", fold_only), 0);
    }

    #[test]
    fn deferral_depends_on_locale_names() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        let lazy = |pairs| Lazy::new(KeySpec::default(), &env(pairs)).is_some();
        assert!(lazy(&[]));
        assert!(lazy(&[("LANG", "en_US.UTF-8")]));
        assert!(lazy(&[("LC_ALL", "POSIX"), ("LANG", "ja_JP.SJIS")]));
        assert!(!lazy(&[("LC_ALL", ""), ("LANG", "ja_JP.SJIS")]));
        assert!(!lazy(&[("LC_COLLATE", "kk_KZ.PT154")]));
        assert!(!lazy(&[("LC_CTYPE", "en_US.ISO8859-1")]));
    }

    #[test]
    fn ascii_lines_never_set_the_locale_up() {
        let spec = KeySpec {
            fields: 1,
            chars: 1,
            fold: true,
        };
        let lazy = Lazy::new(spec, &|_| None).unwrap();
        let (a, b) = (b"1 xAb".as_slice(), b"22 XaB".as_slice());
        let (ai, bi) = (lazy.analyze(a), lazy.analyze(b));
        assert!(lazy.equal(a, ai, b, bi));
        let c = b"3 xAc".as_slice();
        assert!(!lazy.equal(a, ai, c, lazy.analyze(c)));
        let plain = Lazy::new(KeySpec::default(), &|_| None).unwrap();
        let k = Key::default();
        assert!(plain.equal(b"x", k, b"x", k) && !plain.equal(b"x", k, b"y", k));
        assert!(lazy.full.get().is_none() && plain.full.get().is_none());
    }

    #[test]
    fn utf8_decoding() {
        let s = "aé€😀".as_bytes();
        assert_eq!(utf8_at(s, 0), ('a' as u32, 1));
        assert_eq!(utf8_at(s, 1), ('é' as u32, 2));
        assert_eq!(utf8_at(s, 3), ('€' as u32, 3));
        assert_eq!(utf8_at(s, 6), ('😀' as u32, 4));
    }

    fn utf8(spec: KeySpec) -> Utf8Keys {
        Utf8Keys::new(spec, Box::new(Ascii::posix()), false, true)
    }

    #[test]
    fn utf8_validity_rules() {
        let keys = utf8(FIELDS1);
        let a = keys.analyze(b"x \xff");
        let b = keys.analyze(b"y \xff");
        assert!(!a.valid && !b.valid);
        assert!(!keys.equal(b"x \xff", a, b"y \xff", b));
        assert!(keys.equal(b"x \xff", a, b"x \xff", a));
        let c = keys.analyze(b"z \xc3\xa9");
        assert!(c.valid);
        assert!(!keys.equal(b"z \xc3\xa9", c, b"x \xff", a));
    }

    #[test]
    fn fold_comparisons() {
        let keys = utf8(KeySpec {
            fold: true,
            ..KeySpec::default()
        });
        assert!(!keys.fold_eq(b"ab", b"a"));
        assert!(!keys.fold_eq("é".as_bytes(), "éx".as_bytes()));
        assert!(keys.fold_eq(b"AbC", b"aBc"));
        assert!(keys.fold_eq("xé".as_bytes(), "xé".as_bytes()));
    }

    #[test]
    fn without_a_collation_table_different_keys_differ() {
        let keys = utf8(KeySpec::default());
        let k = Key::default();
        assert!(keys.equal(b"same", k, b"same", k));
        assert!(!keys.equal("ж".as_bytes(), k, "я".as_bytes(), k));
        let mut w = Vec::new();
        keys.widen("aж".as_bytes(), &mut w);
        assert_eq!(w, ['a' as Wide, 'ж' as Wide, 0]);
    }
}
