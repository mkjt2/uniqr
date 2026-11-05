#!/usr/bin/env bash
# Cut a release from this machine.
#
# Usage: scripts/release.sh [--dry-run] VERSION
#
# Before running, describe the changes under `## [Unreleased]` in CHANGELOG.md.
#
# Steps:
#   1. Preconditions: valid new VERSION, clean tree, gh authenticated, release
#      tools installed, a non-empty [Unreleased] changelog section, `just ci` passes.
#      VERSION may equal the version in Cargo.toml only for the first release (no
#      v* tags yet).
#   2. Set VERSION in Cargo.toml + Cargo.lock and date the changelog section, in
#      the working tree.
#   3. Build archives for every target (scripts/dist.sh --require-all). If this or
#      anything before it fails, Cargo.toml, Cargo.lock and CHANGELOG.md are put
#      back as they were: nothing is committed or tagged.
#   4. Commit, tag vVERSION, push, create the GitHub release with the archives and
#      install.sh. A failure from here on keeps the commit and tag and prints the
#      remaining steps to run by hand.
#   5. If cli.toml sets [project].homebrew_tap, update the formula in the tap.
#
# --dry-run runs steps 1-3 for real (CI, the version bump, a full build into a
# temporary directory instead of dist/), restores the files, and prints the
# commands it would run after that. It skips the clean-tree and gh auth checks.
#
# RELEASE_COMMIT_TRAILERS adds trailers to the release commit: one `Key: value`
# per line, e.g.
#   RELEASE_COMMIT_TRAILERS=$'Co-Authored-By: Name <email>\nSigned-off-by: Name <email>' just release 1.2.0
source "$(dirname "$0")/lib.sh"

dry_run=false
if [[ "${1:-}" == "--dry-run" ]]; then dry_run=true; shift; fi
run() { if $dry_run; then echo "+ $*" >&2; else "$@"; fi; }

next="${1:-}"
next="${next#v}"
[[ -n "$next" ]] || die "usage: just release VERSION   (current version: $VERSION)"
[[ "$next" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "invalid version '$next' (expected e.g. 1.2.0)"
if [[ "$next" == "$VERSION" && -n "$(git tag -l 'v*')" ]]; then
  die "version is already $VERSION; pick the next one"
fi
if git rev-parse -q --verify "refs/tags/v$next" >/dev/null; then die "tag v$next already exists"; fi

trailers=()
while IFS= read -r line; do
  line="${line#"${line%%[![:space:]]*}"}"
  line="${line%"${line##*[![:space:]]}"}"
  if [[ -n "$line" ]]; then
    [[ "$line" =~ ^[A-Za-z0-9-]+:\ *[^[:space:]] ]] \
      || die "RELEASE_COMMIT_TRAILERS: '$line' is not a 'Key: value' trailer"
    trailers+=(--trailer "$line")
  fi
done <<<"${RELEASE_COMMIT_TRAILERS:-}"

for tool in gh cargo-zigbuild zig; do
  command -v "$tool" >/dev/null || die "$tool is required for releases (see README: Development)"
done
$dry_run || [[ -z "$(git status --porcelain)" ]] || die "working tree is not clean"
$dry_run || gh auth status >/dev/null 2>&1 || die "run 'gh auth login' first"

notes="$(mktemp)"
unreleased_notes >"$notes"
grep -q '[^[:space:]]' "$notes" \
  || die "CHANGELOG.md has nothing under '## [Unreleased]'; describe the changes first"

tap="$(project_value homebrew_tap)"
info "releasing $NAME $VERSION -> $next${tap:+ (homebrew: $tap)}"
if $dry_run; then
  DIST_DIR="$(mktemp -d)"
else
  DIST_DIR=dist # homebrew.sh reads dist/SHA256SUMS
fi
export DIST_DIR

info "running just ci"
just ci

# Bump and build in the working tree; until the commit, any failure (or Ctrl-C)
# puts the three files back.
bumped=(Cargo.toml Cargo.lock CHANGELOG.md)
backup="$(mktemp -d)"
cp "${bumped[@]}" "$backup/"
restore() {
  $dry_run || git reset -q -- "${bumped[@]}"
  (cd "$backup" && cp "${bumped[@]}" "$ROOT/")
  rm -rf "$backup"
  if $dry_run; then
    info "dry run: restored ${bumped[*]}; archives are in $DIST_DIR"
  else
    info "release stopped before committing: restored ${bumped[*]}; nothing was committed, tagged or published"
  fi
}
trap restore EXIT
trap 'exit 130' INT TERM

info "bumping version and dating the changelog"
perl -0pi -e "s/^version = \"[^\"]*\"/version = \"$next\"/m" Cargo.toml
cargo update --workspace --offline
perl -0pi -e "s/^## \\[Unreleased\\]\\n/## [Unreleased]\\n\\n## [$next] - $(date +%Y-%m-%d)\\n/m" CHANGELOG.md
VERSION="$next"

info "building release archives into $DIST_DIR"
"$(dirname "$0")/dist.sh" --require-all

run git add "${bumped[@]}"
run git commit -m "release: v$next" ${trailers[@]+"${trailers[@]}"}

# Past the commit nothing is rolled back: on failure, list what's left to run.
finish=(
  "git tag -a v$next -m v$next"
  "git push --follow-tags"
  "gh release create v$next $DIST_DIR/*.tar.gz $DIST_DIR/*.sha256 $DIST_DIR/SHA256SUMS $DIST_DIR/install.sh --title v$next --notes-file $notes"
)
[[ -z "$tap" ]] || finish+=("scripts/homebrew.sh")
finished=0
left_to_run() {
  echo "error: release v$next stopped after the release commit; nothing was rolled back." >&2
  echo "Fix the cause, then run the remaining steps by hand:" >&2
  printf '  %s\n' "${finish[@]:$finished}" >&2
}
if ! $dry_run; then
  rm -rf "$backup"
  trap left_to_run EXIT
fi

run git tag -a "v$next" -m "v$next"
finished=1
run git push --follow-tags
finished=2
run gh release create "v$next" "$DIST_DIR"/*.tar.gz "$DIST_DIR"/*.sha256 "$DIST_DIR/SHA256SUMS" "$DIST_DIR/install.sh" \
  --title "v$next" --notes-file "$notes"
finished=3
if [[ -n "$tap" ]]; then
  run "$(dirname "$0")/homebrew.sh"
fi

if $dry_run; then
  info "dry run complete: v$next builds; nothing was committed, tagged or published"
else
  trap - EXIT
  info "released v$next"
fi
