#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
paths='fuzz/corpus fuzz/artifacts target/fuzz-artifacts'
status=0

changes=$(git status --porcelain=v1 --untracked-files=all -- $paths)
if [ -n "$changes" ]; then
    echo "fuzz corpus/artifact trees contain tracked or untracked changes:" >&2
    printf '%s\n' "$changes" >&2
    status=1
fi
ignored=$(git ls-files --others --ignored --exclude-standard -- $paths)
if [ -n "$ignored" ]; then
    echo "fuzz corpus/artifact trees contain ignored files:" >&2
    printf '%s\n' "$ignored" >&2
    status=1
fi
tracked_artifacts=$(git ls-files -- fuzz/artifacts target/fuzz-artifacts)
if [ -n "$tracked_artifacts" ]; then
    echo "fuzz artifact trees contain tracked files:" >&2
    printf '%s\n' "$tracked_artifacts" >&2
    status=1
fi

exit "$status"
