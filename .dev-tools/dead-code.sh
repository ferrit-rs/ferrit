#!/bin/sh
# What does rustc say nothing calls?
#
# `dead_code` never fires on a `pub` item of a library: it is exported, so it
# counts as used. This compiles the crate as a binary only, in a throwaway copy
# (no lib.rs; main.rs becomes the crate root), where rustc judges every item
# by whether `main` can reach it, `pub` or not.
#
# Read the list with care. It holds three kinds of items:
#   - dead: nothing else mentions the name (grep src tests examples): delete it;
#   - test seams: only tests/, examples/ or src/replay (the `test-util` harness)
#     call it: keep it;
#   - vendored (src/components/tui_overlay): keep it, it mirrors upstream.
#
#   .dev-tools/dead-code.sh
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
work=${TMPDIR:-/tmp}/ferrit-deadcode

rm -rf "$work/src"
mkdir -p "$work"
cp -R "$root/src" "$work/src"
cp "$root/Cargo.toml" "$root/Cargo.lock" "$root/rust-toolchain.toml" "$root/clippy.toml" "$work/"
rm "$work/src/lib.rs"
{
    printf 'mod app;\nmod components;\nmod config;\nmod git;\nmod keybindings;\nmod theme;\n\n'
    sed 's/ferrit::/crate::/g' "$root/src/main.rs"
} > "$work/src/main.rs"
# The dev-dependency on this crate points at a library that no longer exists here.
grep -v '^ferrit = ' "$work/Cargo.toml" > "$work/Cargo.toml.new"
mv "$work/Cargo.toml.new" "$work/Cargo.toml"

cd "$work"
CARGO_TARGET_DIR="$work/target" cargo check 2>&1 | awk '
    # rustc lists the items of a "multiple ... never used" group one per line.
    /^warning/ { group = 0 }
    /^warning: multiple .*never used/ { print; group = 1; next }
    group && /^ *[0-9]+ *\| *(pub|fn|const)/ { sub(/^ *[0-9]+ *\| */, "    "); print; next }
    /^warning: .*(never used|never constructed|never read|are never)/ { print; show = 3; next }
    show > 0 && /-->/ { print; show = 0; next }
    show > 0 { show-- }
'
