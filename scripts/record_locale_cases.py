#!/usr/bin/env python3
"""Record locale-dependent expectations from the reference uniq.

cli.toml [[case]]s are TOML strings, so they cannot carry invalid UTF-8 or
other encodings. These cases can: each is run through the reference command
(macOS /usr/bin/uniq by default) and written to tests/data/locale_cases.rs as
literal byte expectations, which tests/locale.rs checks runiq against.

Usage: scripts/record_locale_cases.py [--with /usr/bin/uniq]
Re-run after editing CASES and review the diff.
"""
import argparse
import os
import subprocess
import sys

# (name, LC_ALL or None for "inherit nothing", args, input bytes, portable)
# portable=False: the answer comes from macOS libc tables (towlower/iswblank
# for exotic characters, single-byte and legacy multibyte locales) and the
# case only runs on macOS.
U8 = "en_US.UTF-8"
CASES = [
    # Byte-mode comparison (no -f/-s/-i) never decodes.
    ("invalid-utf8-plain", U8, [], b"\xff\n\xff\n\xfe\n", True),
    ("c-locale-high-bytes-fold-ascii", "C", ["-ic"], b"\xffA\n\xffa\n", True),
    ("c-locale-skip-chars-bytes", "C", ["-s1", "-c"], "éx\nax\n".encode(), True),
    ("c-locale-latin1-letters-do-not-fold", "C", ["-ic"], b"\xc9\n\xe9\n", True),
    # UTF-8: decoding, folding, blanks, characters.
    ("utf8-fold-latin", U8, ["-ic"], "Éclair\néclair\nÆ\næ\n".encode(), True),
    ("utf8-fold-dotted-i", U8, ["-ic"], "İ\ni\nI\n".encode(), True),
    ("utf8-fold-kelvin-to-ascii", U8, ["-ic"], "K\nk\nK\n".encode(), True),
    ("utf8-fold-greek", U8, ["-ic"], "Σ\nσ\nς\n".encode(), False),
    ("utf8-fold-cyrillic", U8, ["-ic"], "Ж\nж\nя\nЯ\n".encode(), True),
    ("utf8-nfc-vs-nfd", U8, ["-c"], "é\né\n".encode(), True),
    ("utf8-skip-chars-counts-chars", U8, ["-s1", "-c"], "éx\nax\n😀x\n".encode(), True),
    ("utf8-skip-chars-past-end", U8, ["-s9", "-c"], "é\nab\n".encode(), True),
    ("utf8-nbsp-is-blank", U8, ["-f1", "-c"], "a x\nb x\n".encode(), True),
    ("utf8-ideographic-space-is-blank", U8, ["-f1", "-c"], "a　x\nb　x\n".encode(), True),
    ("utf8-em-space-is-blank", U8, ["-f1", "-c"], "a x\nb x\n".encode(), False),
    ("utf8-zwsp-not-blank", U8, ["-f1", "-c"], "a​x\nb​x\n".encode(), False),
    ("utf8-fields-then-chars", U8, ["-f1", "-s1", "-ic"], "1 éA\n2 xa\n3 yb\n".encode(), True),
    ("utf8-fold-after-skip", U8, ["-s2", "-ic"], "xxÉ\nyyé\n".encode(), True),
    # Invalid sequences: both invalid compare raw (ignoring -f/-s/-i), mixed differ.
    ("utf8-both-invalid-ignore-flags", U8, ["-ic"], b"\xffA\n\xffa\n", True),
    ("utf8-both-invalid-raw-equal", U8, ["-ic"], b"\xffA\n\xffA\n", True),
    ("utf8-both-invalid-skip-ignored", U8, ["-f1", "-c"], b"a \xff\nb \xff\n", True),
    ("utf8-invalid-vs-valid", U8, ["-s1", "-c"], b"\xffx\nax\n", True),
    ("utf8-surrogate-invalid", U8, ["-ic"], b"X\xed\xa0\x80\nx\xed\xa0\x80\n", True),
    ("utf8-overlong-invalid", U8, ["-ic"], b"X\xc0\xaf\nx\xc0\xaf\n", True),
    ("utf8-above-max-invalid", U8, ["-ic"], b"X\xf4\x90\x80\x80\nx\xf4\x90\x80\x80\n", True),
    ("utf8-truncated-sequence", U8, ["-ic"], b"X\xc3\nx\xc3\n", True),
    ("utf8-invalid-after-nul-ignored", U8, ["-ic"], b"A\x00\xff\na\x00\xfe\n", True),
    ("utf8-noncharacter-valid", U8, ["-ic"], "X￿\nx￿\n".encode(), True),
    # Collation: wcscoll ignores characters LC_COLLATE does not define, in
    # every mode (no flags needed).
    ("collate-cyrillic-plain", U8, ["-c"], "ж\nя\nЖ\n".encode(), False),
    ("collate-cjk-emoji-euro", U8, ["-c"], "中\n国\n😀\n€\n".encode(), False),
    ("collate-undefined-ignored", U8, ["-c"], "aж\na\nb\n".encode(), False),
    ("collate-undefined-vs-empty", U8, ["-c"], "ж\n\n\n".encode(), False),
    ("collate-defined-latin-differ", U8, ["-c"], "é\nè\nā\nē\n".encode(), False),
    ("collate-greek-fold", U8, ["-ic"], "Σ\nσ\nς\nω\n".encode(), False),
    ("collate-after-skip", U8, ["-f1", "-c"], "1 ж\n2 я\n3 x\n".encode(), False),
    ("collate-ascii-distinct", U8, ["-c"], b"a\nA\na \na\t\n-a\na-\n", True),
    ("collate-c-utf8-no-table", "C.UTF-8", ["-c"], "ж\nя\n".encode(), True),
    ("collate-c-locale-bytes", "C", ["-c"], "ж\nя\n".encode(), True),
    ("collate-latin1-plain", "en_US.ISO8859-1", ["-c"], b"\xc9\n\xe9\n\xff\n\xfe\n", False),
    ("collate-single-byte-ascii-collision", "kk_KZ.PT154", ["-c"], b"a\x01b\nab\n", False),
    ("collate-sjis-ascii-ignorable", "ja_JP.SJIS", ["-c"], b"a\\b\nab\na~b\n", False),
    ("collate-fold-after-skip-ascii", U8, ["-i", "-f1", "-c"], b"1 Ab\n2 aB\n3 ac\n", True),
    ("collate-invalid-utf8-plain", U8, ["-c"], b"\xffa\n\xffb\n" + "ж\n".encode() + b"\xffa\n", True),
    # Locale selection.
    ("c-utf8-locale", "C.UTF-8", ["-ic"], "É\né\n".encode(), True),
    ("unknown-locale-falls-back-to-c", "xx_YY.UTF-8", ["-ic"], "É\né\n".encode(), False),
    # Single-byte and legacy multibyte locales (macOS only).
    ("latin1-fold", "en_US.ISO8859-1", ["-ic"], b"\xc9\n\xe9\n", False),
    ("latin1-nbsp-blank", "en_US.ISO8859-1", ["-f1", "-c"], b"a\xa0x\nb\xa0x\n", False),
    ("koi8r-fold", "ru_RU.KOI8-R", ["-ic"], b"\xf6\n\xd6\n", False),
    ("sjis-skip-chars", "ja_JP.SJIS", ["-s1", "-c"], b"\x82\xa0x\nax\n", False),
    ("sjis-fold-fullwidth", "ja_JP.SJIS", ["-ic"], b"\x82\x60\n\x82\x81\n", False),
    ("sjis-invalid-raw", "ja_JP.SJIS", ["-ic"], b"\x82A\n\x82a\n", False),
    ("eucjp-skip-chars", "ja_JP.eucJP", ["-s1", "-c"], b"\xa4\xa2x\nax\n", False),
    ("gb18030-four-byte", "zh_CN.GB18030", ["-s1", "-c"], b"\x81\x30\x81\x30x\nax\n", False),
]


def rust_bytes(b: bytes) -> str:
    out = []
    for c in b:
        if c == 0x22:
            out.append('\\"')
        elif c == 0x5C:
            out.append("\\\\")
        elif c == 0x0A:
            out.append("\\n")
        elif 0x20 <= c < 0x7F:
            out.append(chr(c))
        else:
            out.append(f"\\x{c:02x}")
    return 'b"' + "".join(out) + '"'


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--with", dest="ref", default="/usr/bin/uniq")
    a = ap.parse_args()
    lines = [
        "// @generated by scripts/record_locale_cases.py from the reference uniq; do not edit.",
        "/// (name, LC_ALL, args, stdin, expected stdout, expected exit code, portable)",
        "pub type Case = (&'static str, &'static str, &'static [&'static str], &'static [u8], &'static [u8], i32, bool);",
        "",
        "pub const CASES: &[Case] = &[",
    ]
    for name, loc, args, data, portable in CASES:
        env = {k: v for k, v in os.environ.items() if not k.startswith("LC_") and k != "LANG"}
        env["LC_ALL"] = loc
        p = subprocess.run([a.ref, *args], input=data, capture_output=True, env=env)
        argl = ", ".join(f'"{x}"' for x in args)
        lines.append(
            f'    ("{name}", "{loc}", &[{argl}], {rust_bytes(data)}, '
            f"{rust_bytes(p.stdout)}, {p.returncode}, {str(portable).lower()}),"
        )
    lines.append("];")
    path = os.path.join(os.path.dirname(__file__), "..", "tests", "data", "locale_cases.rs")
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {len(CASES)} cases to tests/data/locale_cases.rs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
