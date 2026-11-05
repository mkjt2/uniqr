#!/usr/bin/env python3
"""Differential fuzzer: runiq against the reference uniq (macOS /usr/bin/uniq).

Each case draws arguments (every flag in cli.toml, in all the syntaxes BSD
getopt accepts, plus obsolete forms, operands and deliberate mistakes), an
input built to hit uniq's edge cases (adjacent near-duplicates, case, blanks,
NUL, missing final newline, non-ASCII, invalid UTF-8) and a locale. Both
programs run with the same input, args and environment; runiq is invoked
through a symlink named `uniq` so even stderr must match byte for byte.
Compared: stdout, stderr, exit status, and the OUTPUT file when one is given.

Usage: scripts/fuzz.py [--cases N] [--seed S] [--with /usr/bin/uniq] [--bin target/release/runiq]
Prints the seed; a mismatch is shrunk and printed as a reproducible case.
"""
import argparse
import os
import random
import shutil
import subprocess
import sys
import tempfile

LOCALES = [
    "en_US.UTF-8", "en_US.UTF-8", "en_US.UTF-8", "C", "C", "C.UTF-8",
    "en_US.ISO8859-1", "ja_JP.SJIS", "kk_KZ.PT154", "ru_RU.UTF-8",
    "tr_TR.UTF-8", "de_DE.UTF-8", "fr_FR.UTF-8", "xx_YY.UTF-8",
]

PIECES = [
    b"a", b"A", b"b", b"B", b"x", b"I", b"i", b"z", b"0", b"1", b"9", b" ", b"  ", b"\t",
    b"-", b"+", b".", b",", b"\\", b"~", b"\x01", b"\r", b"\x0b",
    "é".encode(), "É".encode(), "é".encode(), "ж".encode(), "я".encode(),
    "Ж".encode(), "Σ".encode(), "ς".encode(), "σ".encode(), "中".encode(), "😀".encode(),
    " ".encode(), "　".encode(), " ".encode(), "İ".encode(), "ı".encode(),
    "K".encode(), "ß".encode(), "ﬁ".encode(),
    b"\xff", b"\xc3", b"\x82\xa0", b"\x82A", b"\xe9", b"\xc9", b"\x00",
]


def gen_line(r):
    n = r.choice([0, 1, 1, 2, 3, 4, 6, 10])
    return b"".join(r.choice(PIECES) for _ in range(n))


def mutate(r, line):
    """A near-duplicate: same, or differing in case/one piece/prefix."""
    choice = r.random()
    if choice < 0.45:
        return line
    if choice < 0.6:
        return line.swapcase()
    if choice < 0.75 and line:
        i = r.randrange(len(line))
        return line[:i] + r.choice(PIECES) + line[i + 1:]
    if choice < 0.85:
        return r.choice(PIECES) + line
    return gen_line(r)


def gen_input(r):
    lines, prev = [], gen_line(r)
    for _ in range(r.choice([0, 1, 2, 3, 5, 8, 12, 20])):
        prev = mutate(r, prev) if r.random() < 0.7 else gen_line(r)
        lines.append(prev)
    data = b"\n".join(lines)
    if lines and r.random() < 0.8:
        data += b"\n"
    return data


def num(r):
    return r.choice(["0", "1", "1", "2", "3", "5", " 1", "+2", "-0", "007", "", "x", "1x",
                     "2147483647", "2147483648", "-1", " -3", "99999999999999999999"])


def gen_args(r):
    args = []
    shorts = []
    for _ in range(r.choice([0, 1, 1, 2, 2, 3, 4])):
        kind = r.random()
        if kind < 0.35:
            shorts.append(r.choice("cdiu"))
        elif kind < 0.45:
            args.append(r.choice(["--count", "--repeated", "--unique", "--ignore-case",
                                  "--co", "--rep", "--uni", "--ig", "--i", "--u"]))
        elif kind < 0.6:
            sep = r.choice(["", "", "none", "prepend", "separate", "PREPEND", "Sep", "p", "bogus"])
            if r.random() < 0.6:
                args.append("-D" + sep)
            else:
                args.append("--all-repeated" + ("=" + sep if sep or r.random() < 0.3 else ""))
        elif kind < 0.85:
            letter = r.choice("fs")
            v = num(r)
            form = r.random()
            if form < 0.3:
                args += ["-" + letter, v]
            elif form < 0.5:
                args.append("-" + letter + v)
            elif form < 0.7:
                args.append(("--skip-fields" if letter == "f" else "--skip-chars") + "=" + v)
            elif form < 0.8:
                args += ["--skip-" + ("f" if letter == "f" else "c"), v]
            else:
                args.append(("-" if letter == "f" else "+") + r.choice(["1", "2", "0", "3x"]))
        else:
            args.append(r.choice(["-x", "--nope", "--s", "--count=1", "-", "--", "-c-count",
                                  "-c-", "-:", "--help", "--version", "-h", "--=x"]))
    if shorts:
        args.insert(r.randrange(len(args) + 1), "-" + "".join(shorts))
    # Help/version are runiq extensions; the original rejects them.
    args = [a for a in args if a not in ("--help", "--version")]
    operands = []
    roll = r.random()
    if roll < 0.35:
        operands = ["in.txt"]
    elif roll < 0.45:
        operands = ["-"]
    elif roll < 0.55:
        operands = ["in.txt", "out.txt"]
    elif roll < 0.58:
        operands = ["in.txt", "out.txt", "extra"]
    elif roll < 0.6:
        operands = ["missing.txt"]
    if operands and r.random() < 0.2:
        operands.insert(0, "--")
    if operands and r.random() < 0.1:
        operands.append("-c")
    return args + operands


def run(binary, args, data, locale, workdir):
    for f in ("out.txt", "-c"):
        p = os.path.join(workdir, f)
        if os.path.exists(p):
            os.remove(p)
    with open(os.path.join(workdir, "in.txt"), "wb") as f:
        f.write(data)
    env = {k: v for k, v in os.environ.items()
           if not k.startswith("LC_") and k not in ("LANG", "POSIXLY_CORRECT")}
    env["LC_ALL"] = locale
    try:
        p = subprocess.run([binary, *args], input=data, capture_output=True, cwd=workdir,
                           env=env, timeout=10)
    except subprocess.TimeoutExpired:
        return ("timeout",)
    outputs = []
    for f in ("out.txt", "-c"):
        path = os.path.join(workdir, f)
        outputs.append(open(path, "rb").read() if os.path.exists(path) else None)
    return (p.returncode, p.stdout, p.stderr, *outputs)


def shrink(check, args, data, locale):
    """Greedy: drop args, then lines, then bytes, keeping the mismatch."""
    changed = True
    while changed:
        changed = False
        for i in range(len(args)):
            cand = args[:i] + args[i + 1:]
            if check(cand, data, locale):
                args, changed = cand, True
                break
        lines = data.split(b"\n")
        for i in range(len(lines)):
            cand = b"\n".join(lines[:i] + lines[i + 1:])
            if cand != data and check(args, cand, locale):
                data, changed = cand, True
                break
        for i in range(len(data)):
            cand = data[:i] + data[i + 1:]
            if check(args, cand, locale):
                data, changed = cand, True
                break
    return args, data


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cases", type=int, default=3000)
    ap.add_argument("--seed", type=int, default=None)
    ap.add_argument("--with", dest="ref", default="/usr/bin/uniq")
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    ap.add_argument("--bin", default=os.path.join(root, "target", "release", "runiq"))
    a = ap.parse_args()
    seed = a.seed if a.seed is not None else random.randrange(1 << 32)
    print(f"seed {seed}, {a.cases} cases", flush=True)
    r = random.Random(seed)

    tmp = tempfile.mkdtemp(prefix="runiq-fuzz-")
    links = {}
    for name, target in (("ref", a.ref), ("new", a.bin)):
        d = os.path.join(tmp, name)
        os.mkdir(d)
        os.symlink(os.path.abspath(target), os.path.join(d, "uniq"))
        links[name] = os.path.join(d, "uniq")
    work = os.path.join(tmp, "work")
    os.mkdir(work)

    def differs(args, data, locale):
        return run(links["ref"], args, data, locale, work) != run(links["new"], args, data, locale, work)

    mismatches = 0
    for n in range(a.cases):
        args, data, locale = gen_args(r), gen_input(r), r.choice(LOCALES)
        if differs(args, data, locale):
            mismatches += 1
            args, data = shrink(differs, args, data, locale)
            ref = run(links["ref"], args, data, locale, work)
            new = run(links["new"], args, data, locale, work)
            print(f"MISMATCH #{mismatches} (case {n}): LC_ALL={locale} args={args!r} stdin={data!r}")
            print(f"  uniq : {ref!r}")
            print(f"  runiq: {new!r}", flush=True)
            if mismatches >= 10:
                break
    shutil.rmtree(tmp, ignore_errors=True)
    print(f"done: {mismatches} mismatches in {n + 1} cases (seed {seed})")
    return 1 if mismatches else 0


if __name__ == "__main__":
    sys.exit(main())
