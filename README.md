# uniqr

[![CI](https://github.com/mkjt2/uniqr/actions/workflows/ci.yml/badge.svg)](https://github.com/mkjt2/uniqr/actions/workflows/ci.yml)

A Rust port of the macOS `uniq`: same options, same output, same error messages,
and 5-8x faster on ASCII text.

## Why

Just for fun: to see what the Rust rage is all about. Take a classic Unix
tool, write a compatible port, and see how it compares. The sibling project
[rsort](https://github.com/mkjt2/rsort) does the same for `sort`.

What came out is a drop-in replacement for pipelines on macOS. The speedup is
large on ASCII, small on non-ASCII text (libc's collation dominates there), and
memory use is about the same. If `uniq` isn't a bottleneck for you, the system
one is just fine.

## Benchmarks

Each scenario runs `/usr/bin/uniq` and uniqr on the same generated input in
`en_US.UTF-8`, alternating between them with output to `/dev/null`: 1 warmup and
5 timed runs each, median wall time, and a check that the outputs are
byte-identical. Apple M4 (10 cores, 16 GiB), macOS 27.2 (text_cmds-201). Run it
yourself with `just bench`.

🏆 marks the faster or smaller of the two.

**Time** (lower ratio is better; uniqr is faster on all 9 non-empty inputs, and
startup is a tie at 2 ms):

| Scenario | Flags | uniq | uniqr | uniqr/uniq |
| --- | --- | ---: | ---: | ---: |
| 3M lines, groups of 3 | | 222 ms | 39 ms 🏆 | 0.17× |
| 3M lines, groups of 3 | `-c` | 261 ms | 44 ms 🏆 | 0.17× |
| 5M unique lines | | 624 ms | 81 ms 🏆 | 0.13× |
| 5M unique lines | `-u` | 626 ms | 84 ms 🏆 | 0.13× |
| 5M identical lines | `-c` | 430 ms | 85 ms 🏆 | 0.20× |
| 3M log lines, skip a field | `-f 1 -c` | 414 ms | 77 ms 🏆 | 0.19× |
| 3M log lines, skip characters | `-s 3 -d` | 408 ms | 56 ms 🏆 | 0.14× |
| 2M UTF-8 lines | `-c` | 997 ms | 831 ms 🏆 | 0.83× |
| 2M UTF-8 lines, fold case | `-i -f 1 -c` | 1.21 s | 1.15 s 🏆 | 0.95× |
| Empty input (startup) | | 2 ms | 2 ms | 1.25× |

**Memory** (constant in the input size; lower ratio is better):

| Measure | Scenario | uniq | uniqr | uniqr/uniq |
| --- | --- | ---: | ---: | ---: |
| Peak footprint | 3M lines, groups of 3 | 1.28 MiB | 1.19 MiB 🏆 | 0.93× |
| Peak footprint | 2M UTF-8 lines, fold case | 1.42 MiB | 1.36 MiB 🏆 | 0.96× |
| Peak footprint | Empty input | 1.17 MiB 🏆 | 1.19 MiB | 1.01× |
| Max RSS | 3M lines, groups of 3 | 1.27 MiB 🏆 | 1.84 MiB | 1.46× |
| Max RSS | 2M UTF-8 lines, fold case | 1.45 MiB 🏆 | 2.09 MiB | 1.44× |
| Max RSS | Empty input | 1.17 MiB 🏆 | 1.84 MiB | 1.57× |

Max RSS favors `/usr/bin/uniq` because it lives in the dyld shared cache, whose
pages aren't charged to the process; footprint leaves those shared pages out. An
equivalent C program outside the cache starts at 1.64 MiB.

**Binary size:** 352 KB, against 53 KB for the arm64 slice of `/usr/bin/uniq`.
uniqr's only runtime dependencies are `libc` and `memchr`, and the rest of that
is mostly Rust's standard library.

## Validation

uniqr is ported from Apple's text_cmds-199 and checked against that source's own
tests and against `/usr/bin/uniq` itself:

- **uniq's test suite.** Apple's source
  ([text_cmds](https://github.com/apple-oss-distributions/text_cmds)) ships
  `uniq/tests/uniq_test.sh`. It's vendored unchanged and all 15 cases run
  against uniqr and, to validate the harness, against `/usr/bin/uniq` (see
  [tests/upstream](tests/upstream/README.md)).
- **Fuzzing.** `just fuzz` generates random flags in every syntax BSD
  `getopt_long` accepts, deliberate mistakes, and inputs full of near-duplicate
  lines, NUL bytes and invalid UTF-8, across 11 locale names. uniqr runs through
  a symlink named `uniq`, and stdout, stderr, exit status and output files must
  all match byte for byte. A 10,000-case run on macOS 27.2 has 0 mismatches.
- **Recorded behavior.** 56 cases in [`cli.toml`](cli.toml) and 53 byte-exact
  cases in `tests/data/locale_cases.rs`, recorded from `/usr/bin/uniq`, cover
  invalid UTF-8, legacy encodings and macOS collation.
- **The ASCII fast path.** uniqr skips libc calls for ASCII text. On macOS,
  `tests/collation.rs` re-checks against every installed UTF-8 locale the two
  facts that shortcut relies on.
- **Unit and integration tests**, 98% line coverage, and CI on macOS (where the
  suites above run against the runner's uniq) and on Linux (the static musl
  builds that ship).

Some of what that holds in place:

- BSD `getopt_long` parsing: options stop at the first operand, `-D`'s value
  must be attached (`-Dprepend`), long options abbreviate (`--cou`), and the
  obsolete `-NUM` and `+NUM` work. Usage errors exit 1.
- Lines are C strings: output stops at a NUL byte, as in the original.
- Locale-aware comparison through libc, including its surprises: in macOS UTF-8
  locales, characters the collation doesn't define are ignored, so
  `printf 'ж\nя\n' | uniq` prints one line, and so does `uniqr`.
- SIGPIPE is inherited: it dies quietly on a closed pipe, or reports
  `stdout: Broken pipe` and exits 1 when SIGPIPE is ignored.

## Install

```sh
curl -fsSL https://github.com/mkjt2/uniqr/releases/latest/download/install.sh | sh
```

Prebuilt binaries for macOS and Linux (static) are on each
[release](https://github.com/mkjt2/uniqr/releases). From source:
`cargo install --locked --git https://github.com/mkjt2/uniqr`.

```sh
sort cities.txt | uniqr -c           # count how often each line occurs
uniqr -d -i names.txt                # lines repeated in a row, ignoring case
uniqr -Dseparate -f 1 log.txt        # every repeated group, blank line after each
uniqr --help                         # every flag, with examples and exit codes
```

As with `uniq`, only adjacent lines count as repeats, so sort first to find all
duplicates. To use it under the original name, symlink it: a link named `uniq`
prints exactly what `/usr/bin/uniq` prints, including in error messages.

## Differences from macOS uniq

- `--help` and `--version` exist (exact spelling only; the original rejects
  them).
- Errors use the name it was invoked as: `uniqr: nope: No such file or
  directory`, unless it's invoked through a link named `uniq`.
- Output is flushed before every blocking read, so `tail -f log | uniqr` shows
  lines as they arrive. The original does this only on a terminal.
- The Linux builds are static musl binaries. The C and UTF-8 locales work, but
  there's no collation (`wcscoll` is `wcscmp`), case folding uses musl's own
  Unicode tables, and with no locale variables set the locale is UTF-8 rather
  than C, so non-ASCII results can differ from macOS.

## Development

Needs [rustup](https://rustup.rs), [just](https://just.systems) and
`cargo-llvm-cov` (`brew install just cargo-llvm-cov`). Releases also need `zig`,
`cargo-zigbuild` and `gh` (`brew install zig cargo-zigbuild gh`).

| Task | Command |
| --- | --- |
| Local CI: fmt, clippy, tests, 90% coverage gate, static check | `just ci` |
| Run tests | `just test` |
| Re-render `--help` from `cli.toml` and accept the snapshot | `just snapshots` |
| Benchmark against `/usr/bin/uniq` | `just bench` |
| Fuzz against `/usr/bin/uniq` (`--cases`, `--seed`) | `just fuzz` |
| Re-record the byte-exact locale cases | `scripts/record_locale_cases.py` |
| Upgrade toolchain and dependencies | `just upgrade` |

The interface (flags, help text, exit codes) is defined in
[`cli.toml`](cli.toml); `tests/conformance.rs` keeps the code and the prebuilt
`src/help.txt` in line with it. `clap` is a dev-dependency only:
`examples/generate.rs` uses it to render the help, man page and completions, so
it never reaches the binary.

### CI

[GitHub Actions](.github/workflows/ci.yml) runs on every push to `main` and on
pull requests:

| Job | Runner | Runs |
| --- | --- | --- |
| macOS | `macos-latest` | `just ci`, including the comparisons with `/usr/bin/uniq` |
| Differential | `macos-latest` | `just fuzz --cases 2000` with a new random seed each run |
| Linux | `ubuntu-latest` | `just test` on the musl target, `just static-check` |

### Releasing

Releases are cut from a local machine, which builds the macOS and Linux archives:

1. Describe the changes under `## [Unreleased]` in `CHANGELOG.md`, push, and wait
   for CI to pass. The release runs `just ci` again, but not the fuzzer or the
   musl tests.
2. Preview it with `just release --dry-run 1.2.0`, which runs `just ci`, builds
   every archive into a temporary directory, and restores the files it changed.
3. Run `just release 1.2.0`. It bumps the version, dates the changelog, builds
   archives for Linux and macOS on x86_64 and arm64, commits, tags `v1.2.0`,
   pushes, and creates the GitHub release with the archives, checksums and
   `install.sh`, using the changelog section as its notes.

If a step fails before the commit, the files are restored and nothing is
published. If one fails after it, the script prints the remaining commands to
run by hand. To add trailers to the release commit, set
`RELEASE_COMMIT_TRAILERS`; see `scripts/release.sh`.

## License

[Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. uniqr follows
the logic of FreeBSD's uniq (which macOS ships) and `getopt_long`, and vendors
uniq's test suite; see [`NOTICE`](NOTICE).
