#!/bin/sh
set -eu

usage() { echo "usage: $0 little | $0 big TARGET" >&2; exit 2; }
case "$#" in
    1) [ "$1" = little ] || usage; target= ;;
    2) [ "$1" = big ] || usage; target=$2 ;;
    *) usage ;;
esac
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
manifest=$root/target-tests/wide-endian/Cargo.toml
tmp=$(mktemp "${TMPDIR:-/tmp}/zero-schema-wide.XXXXXX")
cleanup() { rm -f "$tmp"; }
terminate() { signal=$1; trap - "$signal"; kill -s "$signal" "$$"; }
trap cleanup EXIT
trap 'terminate HUP' HUP
trap 'terminate INT' INT
trap 'terminate TERM' TERM

run_positive_fixtures() {
    if [ -n "$target" ]; then
        cargo +1.85.0 check --locked --manifest-path "$manifest" --no-default-features --features positive-fixtures --target "$target"
    else
        cargo +1.85.0 test --locked --manifest-path "$manifest" --no-default-features --features positive-fixtures
    fi
}

expect_u16cstr_endian_rejection() {
    feature=$1
    if [ -n "$target" ]; then
        if CARGO_TERM_COLOR=never cargo +1.85.0 check --color never --locked --manifest-path "$manifest" --no-default-features --features "$feature" --target "$target" >"$tmp" 2>&1; then
            echo "$feature unexpectedly compiled" >&2
            exit 1
        fi
    elif CARGO_TERM_COLOR=never cargo +1.85.0 check --color never --locked --manifest-path "$manifest" --no-default-features --features "$feature" >"$tmp" 2>&1; then
        echo "$feature unexpectedly compiled" >&2
        exit 1
    fi

    diagnostic='error: this zero option is not applicable to this string field'
    [ "$(grep -Fxc "$diagnostic" "$tmp")" -eq 1 ] || { echo "$feature did not emit its exact U16CStr diagnostic once" >&2; cat "$tmp" >&2; exit 1; }
    [ "$(grep -c '^error:' "$tmp")" -eq 2 ] || { echo "$feature emitted unexpected additional diagnostics" >&2; cat "$tmp" >&2; exit 1; }
    grep -Fq 'error: could not compile `zero-schema-wide-endian` (lib) due to 1 previous error' "$tmp" || { cat "$tmp" >&2; exit 1; }
}

run_positive_fixtures
for feature in u16cstr-endian-native-reject u16cstr-endian-little-reject u16cstr-endian-big-reject; do
    expect_u16cstr_endian_rejection "$feature"
done
