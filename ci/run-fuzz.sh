#!/bin/sh
set -eu

[ "$#" -eq 1 ] || { echo "usage: $0 TARGET" >&2; exit 2; }
target=$1
case "$target" in parse_message|parse_external_tag|parse_all_strings|roundtrip_message) ;; *) echo "invalid fuzz target: $target" >&2; exit 2;; esac
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
manifest=$root/fuzz/Cargo.toml
lock=$root/fuzz/Cargo.lock
reviewed_corpus=$root/fuzz/corpus/$target
[ -d "$reviewed_corpus" ] || { echo "missing reviewed corpus: $reviewed_corpus" >&2; exit 1; }
[ -f "$lock" ] || { echo "missing fuzz lockfile" >&2; exit 1; }
lock_sha=$(shasum -a 256 "$lock" | awk '{print $1}')
echo "fuzz/Cargo.lock sha256: $lock_sha"
verify_lock() {
    now=$(shasum -a 256 "$lock" | awk '{print $1}')
    [ "$now" = "$lock_sha" ] || { echo "fuzz/Cargo.lock changed during fuzz run" >&2; return 1; }
}
temporary_root=
cleanup() {
    if [ -n "$temporary_root" ]; then
        rm -rf "$temporary_root"
        temporary_root=
    fi
}
finish() {
    status=$?
    trap - EXIT
    cleanup
    if ! verify_lock; then
        status=1
    fi
    exit "$status"
}
terminate() {
    signal=$1
    verify_lock || :
    cleanup
    trap - "$signal"
    kill -"$signal" "$$"
}
trap 'finish' EXIT
trap 'terminate HUP' HUP
trap 'terminate INT' INT
trap 'terminate TERM' TERM

temporary_root=$(mktemp -d "${TMPDIR:-/tmp}/zero-schema-fuzz.XXXXXX")
temporary_corpus=$temporary_root/corpus
mkdir "$temporary_corpus"
cp -R "$reviewed_corpus/." "$temporary_corpus/"

cargo +nightly-2025-11-15 metadata --locked --manifest-path "$manifest" --format-version 1 >/dev/null
artifact_dir=$root/target/fuzz-artifacts/$target/
mkdir -p "$artifact_dir"
case "$artifact_dir" in /*/) ;; *) echo "artifact directory is not absolute and trailing-slash terminated" >&2; exit 1;; esac

# cargo-fuzz does not accept Cargo's --locked flag on `fuzz run`.
(cd "$root/fuzz" && cargo +nightly-2025-11-15 fuzz run "$target" \
    "$temporary_corpus" "$reviewed_corpus" -- \
    -runs=10000 "-artifact_prefix=$artifact_dir")

# A zero exit from libFuzzer is provisional: it may have added, removed, or
# rewritten corpus entries in its first (writable) corpus directory.
comparison=$temporary_root/corpus.diff
if ! diff -r "$reviewed_corpus" "$temporary_corpus" >"$comparison" 2>&1; then
    discovery_root=$artifact_dir/corpus-discoveries
    mkdir -p "$discovery_root"
    discovery_index=1
    while [ -e "$discovery_root/$discovery_index" ]; do
        discovery_index=$((discovery_index + 1))
    done
    discovery_dir=$discovery_root/$discovery_index
    mkdir "$discovery_dir"
    cp -R "$temporary_corpus/." "$discovery_dir/corpus"
    cp "$comparison" "$discovery_dir/diff.txt"
    echo "libFuzzer discovered corpus changes for $target" >&2
    echo "preserved resulting corpus and diff under: $discovery_dir" >&2
    echo "review and commit approved corpus changes before rerunning" >&2
    exit 1
fi
