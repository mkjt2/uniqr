#!/usr/bin/env bash
# Build release archives for every cli.toml target this host can build.
#
#   dist/<name>-<version>-<target>.tar.gz   (binary, completions, man pages, docs)
#   dist/<archive>.sha256 and dist/SHA256SUMS
#
# Usage: scripts/dist.sh [--require-all | --extras-only]
#   --require-all   fail if this host can't build every target
#   --extras-only   only build natively and generate the completions and man pages
#                   into dist/stage/extras (a quick check; no archives)
# DIST_DIR=<dir> writes to <dir> instead of dist/ (it is emptied first).
source "$(dirname "$0")/lib.sh"

require_all=false
extras_only=false
case "${1:-}" in
  "") ;;
  --require-all) require_all=true ;;
  --extras-only) extras_only=true ;;
  *) die "usage: scripts/dist.sh [--require-all | --extras-only]" ;;
esac

dist="${DIST_DIR:-dist}"
case "$dist" in / | . | .. | "$ROOT") die "DIST_DIR=$dist would delete more than build output" ;; esac

targets=()
while IFS= read -r line; do targets+=("$line"); done < <(project_list targets)
[[ ${#targets[@]} -gt 0 ]] || die "no targets in cli.toml [project].targets"

rm -rf "$dist" && mkdir -p "$dist/stage"

# Completions and man pages don't depend on target, and clap stays out of the
# binary: a dev-only example renders them from cli.toml.
info "generating completions and man pages"
generate() { cargo run --quiet --locked --example generate -- "$1"; }
extras="$dist/stage/extras"
mkdir -p "$extras/completions" "$extras/man"
generate complete-bash >"$extras/completions/$NAME.bash"
generate complete-zsh >"$extras/completions/_$NAME"
generate complete-fish >"$extras/completions/$NAME.fish"
generate man >"$extras/man/$NAME.1"
if $extras_only; then
  info "$extras ready"
  exit 0
fi

built=()
for target in "${targets[@]}"; do
  if ! can_build "$target"; then
    $require_all && die "this host cannot build $target (macOS targets need a Mac; Linux targets need zig + cargo-zigbuild). Remove it from [project].targets in cli.toml, or release from a host that can build it"
    info "skipping $target (not buildable on this host)"
    continue
  fi
  info "building $target"
  build_target "$target"
  built+=("$target")
done

package() {
  local target="$1" bin="$2"
  local dir="$NAME-$VERSION-$target"
  mkdir -p "$dist/stage/$dir"
  cp "$bin" "$dist/stage/$dir/$NAME"
  cp -R "$extras/completions" "$extras/man" "$dist/stage/$dir/"
  for f in README.md CHANGELOG.md LICENSE* NOTICE; do [[ -e "$f" ]] && cp "$f" "$dist/stage/$dir/"; done
  tar -C "$dist/stage" -czf "$dist/$dir.tar.gz" "$dir"
  info "packaged $dist/$dir.tar.gz"
}

for target in ${built[@]+"${built[@]}"}; do
  package "$target" "$TARGET_DIR/$target/release/$NAME"
done

cp install.sh "$dist/" 2>/dev/null || true
(
  cd "$dist"
  : >SHA256SUMS
  for f in *.tar.gz; do
    sum="$(sha256 "$f")"
    echo "$sum  $f" >"$f.sha256"
    echo "$sum  $f" >>SHA256SUMS
  done
)
rm -rf "$dist/stage"
info "$dist/ ready: ${#built[@]} target(s)"
