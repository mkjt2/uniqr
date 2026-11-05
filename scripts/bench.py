#!/usr/bin/env python3
"""Benchmark uniqr against macOS /usr/bin/uniq and print Markdown tables.

Usage: scripts/bench.py [options]      (or `just bench [options]`, which builds first)

Each scenario runs both binaries on the same generated input in en_US.UTF-8,
alternating between them with stdout on /dev/null: untimed warmup runs, then
timed runs. It reports the median wall time, max RSS (wait4) and peak memory
footprint (one extra run under /usr/bin/time -l), and checks with one more run
that the outputs are byte-identical. Inputs are deterministic and cached in
target/bench/data. The tables go to stdout and progress to stderr.

Exits 1 if any output differs or a command fails, else 0. Needs macOS and
python3 (3.9+).
"""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import random
import re
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "target" / "bench" / "data"
DATA_VERSION = 1  # bump when a generator changes, to regenerate cached inputs
LOCALE = "en_US.UTF-8"
WORDS = ["апельсин", "яблоко", "груша", "слива", "вишня", "Apple", "apple", "APPLE",
         "banana", "Ελλάδα", "中文", "naïve", "café"]


def gen_utf8():
    r = random.Random(1)
    for i in range(2_000_000):
        w = r.choice(WORDS)
        yield f"{i // 4} {w} {w}\n"


DATASETS = {
    "groups": lambda: (f"{i // 3}\n" for i in range(1, 3_000_001)),
    "unique": lambda: (f"{i}\n" for i in range(1, 5_000_001)),
    "same": lambda: ("the same line over and over\n" for _ in range(5_000_000)),
    "fields": lambda: (f"{i} host{(i // 5) % 1000} GET /index.html 200\n" for i in range(3_000_000)),
    "utf8": gen_utf8,
}

# (name, flags, dataset); no dataset means an empty input, which measures startup.
SCENARIOS = [
    ("3M lines, groups of 3", [], "groups"),
    ("3M lines, groups of 3", ["-c"], "groups"),
    ("5M unique lines", [], "unique"),
    ("5M unique lines", ["-u"], "unique"),
    ("5M identical lines", ["-c"], "same"),
    ("3M log lines, skip a field", ["-f", "1", "-c"], "fields"),
    ("3M log lines, skip characters", ["-s", "3", "-d"], "fields"),
    ("2M UTF-8 lines", ["-c"], "utf8"),
    ("2M UTF-8 lines, fold case", ["-i", "-f", "1", "-c"], "utf8"),
    ("Empty input (startup)", [], None),
]


def dataset(name: str | None) -> str:
    if name is None:
        return "/dev/null"
    path = DATA_DIR / f"{name}-v{DATA_VERSION}.txt"
    if not path.exists():
        print(f"generating {path.name}", file=sys.stderr, flush=True)
        DATA_DIR.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(".tmp")
        with open(tmp, "w", encoding="utf-8") as f:
            f.writelines(DATASETS[name]())
        tmp.replace(path)
    return str(path)


def clean_env() -> dict[str, str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith(("LC_", "LANG"))}
    env["LC_ALL"] = LOCALE
    return env


def timed(cmd: list[str], env: dict[str, str]) -> tuple[float, int]:
    """One run: (wall seconds, max RSS in bytes). Fails on a non-zero exit."""
    start = time.perf_counter()
    proc = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, env=env)
    _, status, usage = os.wait4(proc.pid, 0)
    seconds = time.perf_counter() - start
    proc.returncode = os.waitstatus_to_exitcode(status)
    if proc.returncode != 0:
        raise SystemExit(f"{cmd} exited with {proc.returncode}")
    return seconds, usage.ru_maxrss  # bytes on macOS


def footprint(cmd: list[str], env: dict[str, str]) -> int:
    """Peak memory footprint in bytes, from /usr/bin/time -l."""
    err = subprocess.run(["/usr/bin/time", "-l", *cmd], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.PIPE, env=env, check=True).stderr
    return int(re.search(rb"(\d+)\s+peak memory footprint", err).group(1))


def digest(cmd: list[str], env: dict[str, str]) -> str:
    h = hashlib.sha256()
    with tempfile.TemporaryFile() as out:
        subprocess.run(cmd, stdin=subprocess.DEVNULL, stdout=out, env=env, check=True)
        out.seek(0)
        while chunk := out.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def measure(tools: dict[str, str], flags: list[str], path: str, runs: int, warmup: int) -> dict:
    env = clean_env()
    cmds = {tool: [binary, *flags, path] for tool, binary in tools.items()}
    times: dict[str, list[float]] = {tool: [] for tool in tools}
    rss = dict.fromkeys(tools, 0)
    order = list(tools)
    for i in range(warmup + runs):
        for tool in order if i % 2 == 0 else reversed(order):
            seconds, peak = timed(cmds[tool], env)
            if i >= warmup:
                times[tool].append(seconds)
                rss[tool] = max(rss[tool], peak)
    return {
        "time": {tool: statistics.median(times[tool]) for tool in tools},
        "rss": rss,
        "footprint": {tool: footprint(cmds[tool], env) for tool in tools},
        "same": digest(cmds["uniq"], env) == digest(cmds["uniqr"], env),
    }


def fmt_time(seconds: float) -> str:
    return f"{seconds * 1000:.0f} ms" if seconds < 1 else f"{seconds:.2f} s"


def fmt_mem(n: int) -> str:
    return f"{n / 2**20:.2f} MiB"


def cells(a: float, b: float, fmt) -> list[str]:
    """Both values formatted, 🏆 on the smaller; no trophy if they print the same."""
    x, y = fmt(a), fmt(b)
    if x != y:
        if a < b:
            x += " 🏆"
        else:
            y += " 🏆"
    return [x, y, f"{b / a:.2f}×" if a > 0 else "-"]


def row(name: str, flags: list[str], values: list[str]) -> str:
    shown = f"`{' '.join(flags)}`" if flags else ""
    return "| " + " | ".join([name, shown, *values]) + " |"


def main() -> int:
    parser = argparse.ArgumentParser(description="Benchmark uniqr against /usr/bin/uniq; prints Markdown tables.")
    parser.add_argument("--runs", type=int, default=5, help="timed runs per binary (default 5)")
    parser.add_argument("--warmup", type=int, default=1, help="untimed runs first (default 1)")
    parser.add_argument("--uniq", default="/usr/bin/uniq", help="reference uniq")
    parser.add_argument("--uniqr", default=str(ROOT / "target" / "release" / "uniqr"), help="uniqr binary")
    opts = parser.parse_args()
    if opts.runs < 1 or opts.warmup < 0:
        parser.error("--runs must be at least 1, --warmup at least 0")
    if sys.platform != "darwin":
        parser.error("needs macOS: compares with /usr/bin/uniq and reads /usr/bin/time -l")
    if not Path(opts.uniqr).exists():
        parser.error(f"{opts.uniqr} not found; build it with `cargo build --release` (`just bench` does this)")
    tools = {"uniq": opts.uniq, "uniqr": opts.uniqr}

    results = []
    for i, (name, flags, data) in enumerate(SCENARIOS, 1):
        print(f"[{i}/{len(SCENARIOS)}] {name} {' '.join(flags)}", file=sys.stderr, flush=True)
        results.append((name, flags, measure(tools, flags, dataset(data), opts.runs, opts.warmup)))

    print(f"{platform.machine()}, macOS {platform.mac_ver()[0]}, {LOCALE}; "
          f"{opts.runs} timed runs after {opts.warmup} warmup, median wall time.\n")
    print("Time:\n\n| Scenario | Flags | uniq | uniqr | uniqr/uniq |\n| --- | --- | ---: | ---: | ---: |")
    for name, flags, r in results:
        print(row(name, flags, cells(r["time"]["uniq"], r["time"]["uniqr"], fmt_time)))
    print("\nPeak memory footprint:\n\n| Scenario | Flags | uniq | uniqr | uniqr/uniq |\n| --- | --- | ---: | ---: | ---: |")
    for name, flags, r in results:
        print(row(name, flags, cells(r["footprint"]["uniq"], r["footprint"]["uniqr"], fmt_mem)))
    print("\nMax RSS:\n\n| Scenario | Flags | uniq | uniqr | uniqr/uniq |\n| --- | --- | ---: | ---: | ---: |")
    for name, flags, r in results:
        print(row(name, flags, cells(r["rss"]["uniq"], r["rss"]["uniqr"], fmt_mem)))

    different = [f"{name} {' '.join(flags)}" for name, flags, r in results if not r["same"]]
    for label in different:
        print(f"OUTPUT DIFFERS: {label}", file=sys.stderr)
    print(f"\nOutputs identical in {len(results) - len(different)} of {len(results)} scenarios.")
    return 1 if different else 0


if __name__ == "__main__":
    sys.exit(main())
