#!/usr/bin/env bash
# Compare uniqr with the reference uniq: wall time (best of N), max RSS and
# peak memory footprint (macOS /usr/bin/time -l), and identical output.
#
# Usage: scripts/bench.sh DATA_DIR [RUNS]
# DATA_DIR holds the inputs named in CASES below (see the README's
# Performance section for how they are generated).
set -euo pipefail

dir=${1:?usage: scripts/bench.sh DATA_DIR [RUNS]}
runs=${2:-5}
ref=${REF:-/usr/bin/uniq}
new=${NEW:-$(cd "$(dirname "$0")/.." && pwd)/target/release/uniqr}
export LANG=${LANG:-en_US.UTF-8}

# name|file|args
CASES=(
  "dups (3M lines, groups of 3)|dups3.txt|"
  "dups -c|dups3.txt|-c"
  "all unique (5M lines)|unique.txt|"
  "all unique -u|unique.txt|-u"
  "all same (5M lines)|same.txt|-c"
  "fields -f 1 -c|fields.txt|-f 1 -c"
  "skip chars -s 3 -d|fields.txt|-s 3 -d"
  "utf-8 text -c|utf8.txt|-c"
  "utf-8 text -i -f 1 -c|utf8.txt|-i -f 1 -c"
  "empty input|/dev/null|"
)

measure() { # bin file args... -> "seconds maxrss footprint"
  local bin=$1 file=$2; shift 2
  local best=999999 rss=0 foot=0 t
  for _ in $(seq "$runs"); do
    t=$( { /usr/bin/time -l "$bin" "$@" "$file" >/dev/null; } 2>&1 )
    local real; real=$(awk '/ real /{print $1}' <<<"$t")
    best=$(awk -v a="$best" -v b="$real" 'BEGIN{print (b<a)?b:a}')
    rss=$(awk '/maximum resident set size/{print $1}' <<<"$t")
    foot=$(awk '/peak memory footprint/{print $1}' <<<"$t")
  done
  echo "$best $rss $foot"
}

printf '%-28s %9s %9s %7s  %9s %9s  %9s %9s  %s\n' case "uniq s" "uniqr s" speedup \
  "uniq RSS" "uniqr RSS" "uniq foot" "uniqr foot" output
for c in "${CASES[@]}"; do
  IFS='|' read -r name file args <<<"$c"
  [[ $file == /* ]] || file="$dir/$file"
  # shellcheck disable=SC2086
  if cmp -s <("$ref" $args "$file") <("$new" $args "$file"); then same=identical; else same=DIFFERENT; fi
  # shellcheck disable=SC2086
  read -r rt rr rf <<<"$(measure "$ref" "$file" $args)"
  # shellcheck disable=SC2086
  read -r nt nr nf <<<"$(measure "$new" "$file" $args)"
  speed=$(awk -v a="$rt" -v b="$nt" 'BEGIN{ if (b>0) printf "%.1fx", a/b; else print "-" }')
  printf '%-28s %9s %9s %7s  %8.2fM %8.2fM  %8.2fM %8.2fM  %s\n' "$name" "$rt" "$nt" "$speed" \
    "$(bc -l <<<"$rr/1048576")" "$(bc -l <<<"$nr/1048576")" \
    "$(bc -l <<<"$rf/1048576")" "$(bc -l <<<"$nf/1048576")" "$same"
done
