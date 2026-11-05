# Local CI and release tasks. `just ci` is the canonical CI: it runs fully
# locally; .github/workflows/ci.yml runs it (and more) on every push.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List recipes
default:
    @just --list

# Everything CI checks: format, lints, tests + coverage gate, static linking
ci: fmt-check lint cov static-check

# Format the code
fmt:
    cargo fmt

# Fail if the code is not formatted
fmt-check:
    cargo fmt --check

# Clippy with warnings as errors
lint:
    cargo clippy --locked --all-targets -- -D warnings

# Run the test suite (snapshots must already match)
test *args:
    INSTA_UPDATE=no cargo test --locked {{ args }}

# Tests with coverage; fails below 90% line coverage
cov:
    INSTA_UPDATE=no cargo llvm-cov --locked --fail-under-lines 90 --summary-only

# HTML coverage report in target/llvm-cov/html
cov-html:
    INSTA_UPDATE=no cargo llvm-cov --locked --html

# Re-render src/help.txt from cli.toml and accept the snapshot (review with `git diff`)
snapshots:
    cargo run --quiet --locked --example generate -- help > src/help.txt.new
    mv src/help.txt.new src/help.txt
    INSTA_UPDATE=always cargo test --locked --test help

# Verify Linux musl release binaries are fully static (skipped without zig)
static-check:
    scripts/static-check.sh

# Benchmark uniqr against /usr/bin/uniq; Markdown tables on stdout (`just bench --help`)
bench *args:
    cargo build --locked --release
    scripts/bench.py {{ args }}

# Differential fuzzing against /usr/bin/uniq (`just fuzz --help`)
fuzz *args:
    cargo build --locked --release
    scripts/fuzz.py {{ args }}

# Build release archives for every target this host can build
dist *args:
    scripts/dist.sh {{ args }}

# Release VERSION: tag, build all targets, publish to GitHub (see scripts/release.sh)
release *args:
    scripts/release.sh {{ args }}

# Update the Homebrew tap for the current version
homebrew *args:
    scripts/homebrew.sh {{ args }}

# Upgrade toolchain and dependencies to latest stable, then run CI
upgrade:
    scripts/upgrade.sh

# Install the binary from source into ~/.cargo/bin
install:
    cargo install --locked --path .
