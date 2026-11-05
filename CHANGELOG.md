# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-11-05

### Added
- `uniqr`: a compatible Rust port of macOS `uniq` (text_cmds-199): every option
  including `-D[septype]` and the obsolete `-NUM`/`+NUM`, BSD `getopt_long`
  parsing and messages, locale-aware `-f`/`-s`/`-i` and libc collation, NUL and
  missing-newline quirks, inherited SIGPIPE.
- Long-only `--help` and `--version`.
- Verification: recorded behavior cases, byte-exact locale cases, the vendored
  upstream test suite, and a differential fuzzer (`just fuzz`).
- `just bench`, CI on GitHub Actions for macOS and Linux, license files (MIT and
  Apache-2.0) and a NOTICE crediting the BSD sources.
- See the README for benchmarks and differences from macOS uniq.
