# uniqr

Report or filter out repeated lines in a file: a byte-for-byte compatible
Rust clone of macOS `uniq` (Apple text_cmds-199), faster and leaner.

```sh
sort cities.txt | uniqr -c        # count how often each line occurs
uniqr -d -i names.txt             # lines repeated in a row, ignoring case
uniqr -Dseparate -f 1 log.txt     # every repeated group, blank line after each
```

## Compatibility

Same options (`-c -d -D[septype] -f -i -s -u`, their long forms, the obsolete
`-NUM`/`+NUM`), same output, same error messages and exit status (usage errors
exit 1). Behavior that is easy to get wrong and is reproduced on purpose:

- BSD `getopt_long` parsing: options stop at the first operand, `-D`'s value
  must be attached (`-Dprepend`), long options abbreviate (`--cou`).
- Lines are C strings: output stops at a NUL byte, as in the original.
- Locale-aware comparison through libc: `-i` uses `towlower`, `-f` uses
  `iswblank`, `-s` counts characters, and lines are equal when `wcscoll` says
  so. In macOS UTF-8 locales that means characters the locale's collation does
  not define are ignored: `printf 'ж\nя\n' | uniq` prints one line, and so does
  `uniqr`.
- SIGPIPE is inherited (dies quietly on a closed pipe by default; reports
  `stdout: Broken pipe` and exits 1 when SIGPIPE is ignored).
- Error messages use the name it was invoked as, so a symlink named `uniq`
  prints exactly what `/usr/bin/uniq` prints.

Additions: `--help`, `--version` (exact spelling only; the original rejects
them) and the hidden `--generate man|schema|complete-<shell>`. Output is also
flushed before every blocking read, so `tail -f log | uniqr` shows lines as
they arrive (the original does this only on a terminal).

On Linux (static musl builds) the C and UTF-8 locales behave the same way,
but musl has no collation (`wcscoll` is `wcscmp`) and its own Unicode case
tables, so non-ASCII results can differ from macOS.

### How compatibility is checked

- `cli.toml` behavior cases, recorded from `/usr/bin/uniq`.
- `tests/data/locale_cases.rs`: byte-exact cases recorded from `/usr/bin/uniq`
  (`scripts/record_locale_cases.py`) for invalid UTF-8, legacy encodings and
  macOS collation.
- The original's own test suite, vendored verbatim in `tests/upstream/` and
  run against both `uniqr` and `/usr/bin/uniq` (`tests/upstream.rs`).
- `scripts/fuzz.py`: differential fuzzing against `/usr/bin/uniq` comparing
  stdout, stderr, exit status and output files (`scripts/fuzz.py --cases 10000`).
- `tests/collation.rs` re-checks, against every UTF-8 locale installed, the two
  facts uniqr relies on to skip libc calls for ASCII text.

## Performance

macOS arm64, best of 5 runs, `scripts/bench.sh` (output verified identical). 🏆 marks the better number:

| Workload | Time uniq | Time uniqr | Speedup | Peak footprint uniq | Peak footprint uniqr |
| --- | --- | --- | --- | --- | --- |
| 3M lines, groups of 3 | 0.18 s | 0.03 s 🏆 | 6x | 1.28 MiB | 1.22 MiB 🏆 |
| 5M unique lines, `-u` | 0.39 s | 0.06 s 🏆 | 6.5x | 1.33 MiB | 1.22 MiB 🏆 |
| 3M lines, `-f 1 -c` | 0.41 s | 0.07 s 🏆 | 5.9x | 1.33 MiB | 1.22 MiB 🏆 |
| 2M UTF-8 lines, `-c` | 0.99 s | 0.82 s 🏆 | 1.2x | 1.38 MiB | 1.38 MiB (tie) |
| 2M UTF-8 lines, `-i -f 1 -c` | 1.20 s | 1.14 s 🏆 | 1.1x | 1.42 MiB | 1.38 MiB 🏆 |

Memory is constant in the input size (a 16 KiB read buffer, an 8 KiB output
buffer and a copy of the previous line). Non-ASCII text in a UTF-8 locale is
dominated by libc's `wcscoll`, which both programs call. Maximum RSS is higher
for uniqr (about 2.0-2.3 MiB vs 1.2-1.4 MiB) because `/usr/bin/uniq` lives in
the dyld shared cache, whose pages are not charged to the process. Any binary
outside it starts higher (an empty C program: 1.7 MiB RSS).

Benchmark inputs:

```sh
seq 1 3000000 | awk '{print int($1/3)}' > dups3.txt
seq 1 5000000 > unique.txt
yes 'the same line over and over' | head -5000000 > same.txt
python3 - <<'PY'
import random; random.seed(1)
words = ['апельсин', 'яблоко', 'груша', 'слива', 'вишня', 'Apple', 'apple', 'APPLE',
         'banana', 'Ελλάδα', '中文', 'naïve', 'café']
with open('utf8.txt', 'w') as f:
    for i in range(2000000):
        w = random.choice(words); f.write(f'{i//4} {w} {w}\n')
with open('fields.txt', 'w') as f:
    for i in range(3000000):
        f.write(f'{i} host{(i//5)%1000} GET /index.html 200\n')
PY
scripts/bench.sh .
```

## Install

```sh
curl -fsSL https://github.com/mkjt2/uniqr/releases/latest/download/install.sh | sh
```

Prebuilt binaries for Linux (static musl) and macOS are attached to each
[GitHub release](https://github.com/mkjt2/uniqr/releases).
From source: `cargo install --locked --git https://github.com/mkjt2/uniqr`.

## Usage

```sh
uniqr --help              # human help, with examples and exit codes
uniqr --generate schema   # the full interface as JSON (for scripts and agents)
```

## Development

Requirements: [rustup](https://rustup.rs), [just](https://just.systems) and
`cargo-llvm-cov`. Releases also need `zig`, `cargo-zigbuild` (static Linux builds)
and `gh`.

```sh
brew install just cargo-llvm-cov            # macOS
brew install zig cargo-zigbuild gh          # macOS, for releases
cargo install just cargo-llvm-cov           # Linux (install zig and gh from your package manager)
```

| Task | Command |
| --- | --- |
| Full local CI (fmt, clippy, tests, 90% coverage gate, static check) | `just ci` |
| Run tests | `just test` |
| Accept changed `--help` snapshots | `just snapshots` |
| Build release archives | `just dist` |
| Cut a release (describe it under `[Unreleased]` in CHANGELOG.md first) | `just release 1.2.0` |
| Preview a release (runs CI and builds every archive, changes nothing) | `just release --dry-run 1.2.0` |
| Upgrade toolchain and dependencies | `just upgrade` |
| Differential fuzzing against `/usr/bin/uniq` | `cargo build --release && scripts/fuzz.py` |
| Benchmark against `/usr/bin/uniq` | `cargo build --release && scripts/bench.sh DIR` |
| Re-record the byte-exact locale cases | `scripts/record_locale_cases.py` |

To add trailers to the release commit, set `RELEASE_COMMIT_TRAILERS` (one
`Key: value` per line); see `scripts/release.sh`.

The interface is defined in [`cli.toml`](cli.toml): commands, flags, help text,
exit codes and JSON output schemas. `tests/conformance.rs` fails if the code
drifts from it.
