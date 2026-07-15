#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/zero-schema-run-fuzz-test.XXXXXX")
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT HUP INT TERM

fixture=$tmp/repo
mkdir -p "$fixture/ci" "$fixture/fuzz/corpus/parse_message" "$fixture/fake-bin"
cp "$root/ci/run-fuzz.sh" "$fixture/ci/run-fuzz.sh"
printf '%s\n' '[package]' >"$fixture/fuzz/Cargo.toml"
printf '%s\n' 'reviewed lock' >"$fixture/fuzz/Cargo.lock"
printf '%s\n' 'reviewed bytes' >"$fixture/fuzz/corpus/parse_message/existing"

cat >"$fixture/fake-bin/cargo" <<'FAKE'
#!/bin/sh
set -eu
case " $* " in
  *' metadata '*) exit 0 ;;
  *' fuzz run '*)
    case " $* " in
      *' -seed=424242 '*) ;;
      *) echo 'missing stable libFuzzer seed' >&2; exit 97 ;;
    esac
    corpus=$5
    case "${FAKE_CARGO_ACTION:-unchanged}" in
      unchanged) ;;
      add) printf '%s\n' 'new discovery' >"$corpus/new-entry" ;;
      change) printf '%s\n' 'changed discovery' >"$corpus/existing" ;;
      lock) printf '%s\n' 'mutated lock' >>"$FAKE_REPO/fuzz/Cargo.lock" ;;
      *) exit 99 ;;
    esac
    ;;
  *) exit 98 ;;
esac
FAKE
chmod +x "$fixture/fake-bin/cargo"

run_fuzz() {
    FAKE_CARGO_ACTION=$1 FAKE_REPO=$fixture PATH="$fixture/fake-bin:$PATH" \
        "$fixture/ci/run-fuzz.sh" parse_message >"$tmp/$1.out" 2>&1
}

if "$fixture/ci/run-fuzz.sh" >"$tmp/no-args.out" 2>&1; then
    echo 'missing fuzz target unexpectedly passed' >&2
    exit 1
fi
grep "usage: $fixture/ci/run-fuzz.sh TARGET" "$tmp/no-args.out" >/dev/null

if "$fixture/ci/run-fuzz.sh" invalid >"$tmp/invalid-target.out" 2>&1; then
    echo 'invalid fuzz target unexpectedly passed' >&2
    exit 1
fi
grep 'invalid fuzz target: invalid' "$tmp/invalid-target.out" >/dev/null

run_fuzz unchanged
[ ! -e "$fixture/target/fuzz-artifacts/parse_message/corpus-discoveries" ]

if run_fuzz add; then
    echo 'new corpus entry unexpectedly passed' >&2
    exit 1
fi
add_dir=$fixture/target/fuzz-artifacts/parse_message/corpus-discoveries/1
[ "$(cat "$add_dir/corpus/new-entry")" = 'new discovery' ]
[ "$(cat "$fixture/fuzz/corpus/parse_message/existing")" = 'reviewed bytes' ]
grep 'libFuzzer discovered corpus changes' "$tmp/add.out" >/dev/null

if run_fuzz change; then
    echo 'changed corpus entry unexpectedly passed' >&2
    exit 1
fi
change_dir=$fixture/target/fuzz-artifacts/parse_message/corpus-discoveries/2
[ "$(cat "$change_dir/corpus/existing")" = 'changed discovery' ]
[ "$(cat "$fixture/fuzz/corpus/parse_message/existing")" = 'reviewed bytes' ]

if run_fuzz lock; then
    echo 'lock mutation unexpectedly passed' >&2
    exit 1
fi
grep 'fuzz/Cargo.lock changed during fuzz run' "$tmp/lock.out" >/dev/null

printf '%s\n' 'run-fuzz fake-cargo tests passed'
