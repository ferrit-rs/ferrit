#!/bin/sh
# Turn every script in test/scripts into a `vhs` tape in test/tapes (or
# $TAPES_DIR). The mapping lives in the binary (`ferrit --tape`), next to the
# script parser, so a new directive is added in one place.
#
# A script that changes the repository or the configuration from outside the
# terminal (`exec`, `write`, `config`, `async-key`) cannot be replayed by a real
# terminal session and is skipped with the reason. The binary must be a debug
# build, or FERRIT_TEST must be set: `--tape` is a test-only flag.
#
#   test/gen-tapes.sh && vhs test/tapes/10-layout.tape
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${FERRIT_BIN:-$ROOT/target/debug/ferrit}
OUT=${TAPES_DIR:-$ROOT/test/tapes}
mkdir -p "$OUT"

for script in "$ROOT"/test/scripts/*.script; do
    name=$(basename "$script" .script)
    if "$BIN" --tape "$script" >"$OUT/$name.tape" 2>"$OUT/.error"; then
        echo "tape: $name"
    else
        rm -f "$OUT/$name.tape"
        # The first line that says why (the error is wrapped in report noise).
        reason=$(grep -m1 'line [0-9]*:' "$OUT/.error" | sed 's/\x1b\[[0-9;]*m//g' | sed 's/^ *[0-9]*: *//')
        echo "skip: $name (${reason:-see the script})"
    fi
done
rm -f "$OUT/.error"
