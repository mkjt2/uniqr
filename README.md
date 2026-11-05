# runiq

Report or filter out repeated lines in a file.

## Install

```sh
curl -fsSL https://github.com/mkjt2/runiq/releases/latest/download/install.sh | sh
```

Prebuilt binaries for Linux (static musl) and macOS are attached to each
[GitHub release](https://github.com/mkjt2/runiq/releases).
From source: `cargo install --locked --git https://github.com/mkjt2/runiq`.

## Usage

```sh
runiq --help              # human help, with examples and exit codes
runiq --generate schema   # the full interface as JSON (for scripts and agents)
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

To add trailers to the release commit, set `RELEASE_COMMIT_TRAILERS` (one
`Key: value` per line); see `scripts/release.sh`.

The interface is defined in [`cli.toml`](cli.toml): commands, flags, help text,
exit codes and JSON output schemas. `tests/conformance.rs` fails if the code
drifts from it.
