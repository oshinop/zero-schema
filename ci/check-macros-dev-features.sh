#!/bin/sh
set -eu

[ "$#" -eq 1 ] || { echo "usage: $0 TOOLCHAIN" >&2; exit 2; }
toolchain=$1
case "$toolchain" in *[!A-Za-z0-9._-]*|'') echo "invalid toolchain: $toolchain" >&2; exit 2;; esac
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out=$(mktemp "${TMPDIR:-/tmp}/zero-schema-tree.XXXXXX")
cleanup() { rm -f "$out"; }
terminate() { signal=$1; trap - "$signal"; kill -s "$signal" "$$"; }
trap cleanup EXIT
trap 'terminate HUP' HUP
trap 'terminate INT' INT
trap 'terminate TERM' TERM

cargo "+$toolchain" tree --locked --manifest-path "$root/Cargo.toml" \
    -p zero-schema-macros -e features,dev --prefix depth --format '{p}|{f}' >"$out"

zs=$(awk -F'|' '$1 ~ /^[0-9]+zero-schema v/ { print }' "$out")
[ -n "$zs" ] || { echo "macro dev graph does not contain zero-schema" >&2; cat "$out" >&2; exit 1; }
[ "$(printf '%s\n' "$zs" | wc -l | tr -d ' ')" -eq 1 ] || { echo "zero-schema appears more than once" >&2; cat "$out" >&2; exit 1; }
features=${zs#*|}
[ -z "$features" ] || { echo "macro dev backedge enabled forbidden zero-schema features: $features" >&2; exit 1; }
if grep -E 'zero-schema feature "std"' "$out" >/dev/null 2>&1; then
    echo "macro dev backedge enabled std" >&2
    exit 1
fi
