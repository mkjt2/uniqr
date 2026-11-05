//! Guards the assumptions src/locale.rs takes without asking libc:
//! in a UTF-8 locale, two different all-ASCII keys never collate equal, and
//! ASCII characters fold and classify as blanks exactly as in the C locale. This probes libc directly in every UTF-8
//! locale the system ships, with near-miss pairs (substitution, insertion,
//! deletion, transposition) over all of ASCII, so an OS update that breaks
//! the assumption fails here instead of silently changing output.

#[cfg(target_os = "macos")]
#[test]
fn ascii_never_collides_in_utf8_locales() {
    use std::ffi::CString;

    unsafe extern "C" {
        fn wcscoll(a: *const libc::wchar_t, b: *const libc::wchar_t) -> libc::c_int;
    }

    let out = std::process::Command::new("locale")
        .arg("-a")
        .output()
        .unwrap();
    let locales: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.ends_with(".UTF-8"))
        .map(str::to_string)
        .collect();
    assert!(
        locales.len() > 20,
        "expected many UTF-8 locales: {locales:?}"
    );

    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rand = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let ascii = |r: u64| -> libc::wchar_t {
        // Any ASCII except NUL and newline (which never reach a key).
        let c = 1 + (r % 126) as libc::wchar_t;
        if c == 10 { 11 } else { c }
    };

    let mut collisions = Vec::new();
    for name in &locales {
        let c = CString::new(name.as_str()).unwrap();
        // SAFETY: single-threaded test binary (this is its only test).
        if unsafe { libc::setlocale(libc::LC_ALL, c.as_ptr()) }.is_null() {
            continue;
        }
        let coll = |a: &[libc::wchar_t], b: &[libc::wchar_t]| {
            let (mut a, mut b) = (a.to_vec(), b.to_vec());
            a.push(0);
            b.push(0);
            // SAFETY: NUL-terminated.
            unsafe { wcscoll(a.as_ptr(), b.as_ptr()) }
        };
        // Every pair of single characters, and every character inside a word.
        for x in 1..127 {
            for y in x + 1..127 {
                if x != 10 && y != 10 && coll(&[x], &[y]) == 0 {
                    collisions.push(format!("{name}: {x} == {y}"));
                }
            }
            if x != 10 && coll(&['a' as _, x, 'b' as _], &['a' as _, 'b' as _]) == 0 {
                collisions.push(format!("{name}: {x} is ignorable"));
            }
        }
        for _ in 0..20_000 {
            let n = 1 + rand(6) as usize;
            let a: Vec<libc::wchar_t> = (0..n).map(|_| ascii(rand(1 << 16))).collect();
            let mut b = a.clone();
            let p = rand(n as u64) as usize;
            match rand(4) {
                0 => b[p] = ascii(rand(1 << 16)),
                1 => b.insert(p, ascii(rand(1 << 16))),
                2 => {
                    b.remove(p);
                }
                _ if p + 1 < n => b.swap(p, p + 1),
                _ => {}
            }
            if a != b && coll(&a, &b) == 0 {
                collisions.push(format!("{name}: {a:?} == {b:?}"));
            }
        }
    }
    assert!(collisions.is_empty(), "{}", collisions.join("\n"));
}

#[cfg(target_os = "macos")]
#[test]
fn ascii_ctype_is_the_same_in_every_utf8_locale() {
    use std::process::Command;
    // Run in a child so setlocale here cannot race the other test's.
    let probe = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ascii_ctype_probe", "--nocapture", "--ignored"])
        .output()
        .unwrap();
    assert!(
        probe.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.stdout)
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "run by ascii_ctype_is_the_same_in_every_utf8_locale"]
fn ascii_ctype_probe() {
    use std::ffi::CString;
    unsafe extern "C" {
        fn towlower(c: i32) -> i32;
        fn iswblank(c: i32) -> i32;
    }
    let out = std::process::Command::new("locale")
        .arg("-a")
        .output()
        .unwrap();
    let mut names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.ends_with(".UTF-8"))
        .map(str::to_string)
        .collect();
    names.extend(["C".into(), "POSIX".into()]);
    let mut bad = Vec::new();
    for name in &names {
        let c = CString::new(name.as_str()).unwrap();
        // SAFETY: this ignored test runs alone in its own process.
        if unsafe { libc::setlocale(libc::LC_ALL, c.as_ptr()) }.is_null() {
            continue;
        }
        for b in 0..128u8 {
            let lower = i32::from(b.to_ascii_lowercase());
            let blank = b == b' ' || b == b'\t';
            // SAFETY: plain libc classification calls.
            let (l, k) = unsafe { (towlower(i32::from(b)), iswblank(i32::from(b)) != 0) };
            if l != lower || k != blank {
                bad.push(format!("{name}: {b} lower={l} blank={k}"));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
