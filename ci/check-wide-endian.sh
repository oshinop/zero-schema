#!/bin/sh
set -eu

case "$#:$1" in 1:little) target=;; 2:big) target=$2;; *) echo "usage: $0 little | $0 big TARGET" >&2; exit 2;; esac
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
manifest=$root/target-tests/wide-endian/Cargo.toml
tmp=$(mktemp "${TMPDIR:-/tmp}/zero-schema-wide.XXXXXX")
cleanup() { rm -f "$tmp"; }
terminate() { signal=$1; trap - "$signal"; kill -s "$signal" "$$"; }
trap cleanup EXIT
trap 'terminate HUP' HUP
trap 'terminate INT' INT
trap 'terminate TERM' TERM

for view in u16str u16cstr; do
    for source in explicit inherited; do
        for endian in little big; do
            feature=$source-$endian-$view
            if [ "$endian" = "$1" ]; then
                cargo +1.85.0 check --locked --manifest-path "$manifest" --no-default-features --features "$feature" ${target:+--target "$target"}
                if [ "$1" = little ]; then
                    cargo +1.97.0 clippy --locked --manifest-path "$manifest" --no-default-features --features "$feature" -- -D warnings
                fi
            else
                if CARGO_TERM_COLOR=never cargo +1.85.0 check --color never --locked --manifest-path "$manifest" --no-default-features --features "$feature" ${target:+--target "$target"} >"$tmp" 2>&1; then
                    echo "$feature unexpectedly compiled for a $1-endian target" >&2; exit 1
                fi
                expected="error: wide string wire representation requires a $endian-endian target"
                [ "$(grep -Fxc "$expected" "$tmp")" -eq 1 ] || { echo "$feature did not emit its exact endian diagnostic once" >&2; cat "$tmp" >&2; exit 1; }
                [ "$(grep -c '^error:' "$tmp")" -eq 2 ] || { echo "$feature emitted unexpected additional diagnostics" >&2; cat "$tmp" >&2; exit 1; }
                grep -Fq 'error: could not compile `zero-schema-wide-endian` (lib) due to 1 previous error' "$tmp" || { cat "$tmp" >&2; exit 1; }
            fi
        done
    done
    for inherited in little big; do
        feature=native-over-$inherited-$view
        cargo +1.85.0 check --locked --manifest-path "$manifest" --no-default-features --features "$feature" ${target:+--target "$target"}
        if [ "$1" = little ]; then
            cargo +1.97.0 clippy --locked --manifest-path "$manifest" --no-default-features --features "$feature" -- -D warnings
        fi
    done
done
